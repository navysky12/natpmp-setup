use anyhow::{anyhow, bail, Result};
use natpmp::*;
use reqwest::blocking::Client;
use reqwest::Url;
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    // Retrieve the gateway IP from environment variable or use a default.
    let gateway = std::env::var("NATPMP_GATEWAY_IP").unwrap_or("10.2.0.1".to_owned());
    // Create a new NAT-PMP client using the gateway IP.
    let mut n =
        Natpmp::new_with((&gateway).parse().unwrap()).expect("Parsing gateway address failed!");
    // Initialize a default HTTP client instance for making network requests. 
    let mut client = Client::default();

    // Query the gateway for public IP address, handle failures.
    let _ = query_gateway(&mut n).expect("Querying Public IP failed!");

    // Query for an available port using NAT-PMP.
    let mut mr = query_available_port(&mut n).expect("Querying a Port Mapping failed!");
    // Send API call to qBittorrent client containing port information.
    update_qbittorrent(&mut client, mr.public_port()).expect("Failed to update QBittorrent.");

    // Infinite loop to continuously check and update port mappings.
    loop {
        // Sleep for half the lifetime of the port mapping before renewing.
        thread::sleep(mr.lifetime().clone() / 2);
        // Attempt to renew the port mapping or find a new available port.
        let mr_ = query_port(&mut n, mr.private_port(), mr.public_port(), true)
            .or(query_available_port(&mut n))
            .expect("Every renewal method failed!");
        // Check if the public port has changed.
        if mr.public_port() != mr_.public_port() {
            println!("Port has changed, setting incoming port on QBittorrent...");
            update_qbittorrent(&mut client, mr.public_port())
                .expect("Failed to update QBittorrent.");
        }
        // Update the mapping response to continue with the new or renewed mapping.
        mr = mr_;
    }

    Ok(())
}

// Function to send API call to qBittorrent client with port information.
fn update_qbittorrent(client: &mut Client, port: u16) -> Result<()> {
    client
        .post(Url::parse("http://127.0.0.1:8080/api/v2/app/setPreferences").unwrap())
        .form(&[("json", &format!(r#"{{"listen_port":{}}}"#, port))])
        .send()?
        .error_for_status()?;
    Ok(())
}

// Function to query the gateway for a public IP address.
fn query_gateway(n: &mut Natpmp) -> Result<GatewayResponse> {
    let mut timeout = 250;
    while timeout <= 64000 {
        // Send a public address request to the gateway.
        n.send_public_address_request()
            .map_err(|err| anyhow!("Fail with {:?}", err))?;
        println!(
            "Public address request sent! (will timeout in {}ms)",
            timeout
        );
        // Wait for a response or timeout.
        thread::sleep(Duration::from_millis(timeout));
        match n.read_response_or_retry() {
            Err(e) => match e {
                Error::NATPMP_TRYAGAIN => println!("Try again later"),
                _ => return Err(anyhow!("Try again: {:?}", e)),
            },
            Ok(Response::Gateway(gr)) => {
                // Successfully received a response with the public IP.
                println!(
                    "Got response: IP: {}, Epoch: {}",
                    gr.public_address(),
                    gr.epoch()
                );
                return Ok(gr);
            }
            _ => {
                bail!("Expecting a gateway response");
            }
        };
        // Increase timeout for the next attempt.
        timeout *= 2;
    }
    bail!("Querying gateway failed!");
}

// Function to query an available port using NAT-PMP.
fn query_available_port(n: &mut Natpmp) -> Result<MappingResponse> {
    return query_port(n, 0, 0, false);
}

// Function to request or renew a port mapping.
fn query_port(
    n: &mut Natpmp,
    internal: u16,
    external: u16,
    check: bool,
) -> Result<MappingResponse> {
    let mut timeout = 250;
    while timeout <= 64000 {
        // Send a port mapping request.
        let _ = n.send_port_mapping_request(Protocol::TCP, 0, 0, 360)
            .map_err(|err| anyhow!("Fail with {:?}", err));
        println!("Port mapping request sent! (will timeout in {}ms)", timeout);

        // Wait for a response or timeout.
        thread::sleep(Duration::from_millis(1000));
        match n.read_response_or_retry() {
            Err(e) => match e {
                Error::NATPMP_TRYAGAIN => println!("Try again later"),
                _ => return Err(anyhow!("Try again later: {:?}", e)),
            },
            Ok(Response::TCP(tr)) => {
                // Successfully received a TCP response with port information.
                println!(
                    "Got response: Internal: {}, External: {}, Lifetime: {}s",
                    tr.private_port(),
                    tr.public_port(),
                    tr.lifetime().as_secs()
                );
                // Verify if the response matches the requested mapping, if applicable.
                if (!check)
                    || (tr.private_port() == internal
                        && tr.public_port() == external
                        && tr.lifetime().as_secs() > 0)
                {
                    return Ok(tr);
                } else {
                    println!("Retrying, port is not the one wanted!");
                }
            }
            _ => {
                bail!("Expecting a TCP response");
            }
        };
        // Increase timeout for the next attempt.
        timeout *= 2;
    }
    bail!("Mapping failed!");
}
