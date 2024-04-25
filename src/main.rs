use anyhow::{anyhow, bail, Result};
use natpmp::*;
use std::env;
use std::fs::File;
use std::io::{Write, Result as IoResult};
use std::process;
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    // Retrieve the gateway IP from environment variable or use a default.
    let gateway = env::var("NATPMP_GATEWAY_IP").unwrap_or("10.2.0.1".to_owned());
    // Create a new NAT-PMP client using the gateway IP.
    let mut n =
        Natpmp::new_with((&gateway).parse().unwrap()).expect("Parsing gateway address failed!");

    // Retrieve the first command line argument as the filename for the output file.
    let filename = env::args().nth(1).expect("No file name provided as argument");

    // Open or create the file where PID and port information will be written.
    let mut file = File::create(filename)?;

    // Query the gateway for public IP address, handle failures.
    let _ = query_gateway(&mut n).expect("Querying Public IP failed!");

    // Query for an available port using NAT-PMP.
    let mut mr = query_available_port(&mut n).expect("Querying a Port Mapping failed!");
    // Write the initial PID and port information to the file.
    print_loop_info(&mut file, mr.public_port()).expect("Failed to write loop information.");

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
            println!("Port has changed, updating file...");
            // Update the file with the new port information.
            print_loop_info(&mut file, mr_.public_port())
                .expect("Failed to write loop information.");
        }
        // Update the mapping response to continue with the new or renewed mapping.
        mr = mr_;
    }
}

// Function to write the PID and port information to a file.
fn print_loop_info(file: &mut File, port: u16) -> IoResult<()> {
    let pid = process::id();  // Get the current process ID.
    writeln!(file, "{},{}", pid, port)?;  // Write the PID and port to the file.
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
        let _ = n.send_port_mapping_request(Protocol::TCP, internal, external, 360)
            .map_err(|err| anyhow!("Failed to send port mapping request: {:?}", err));
        println!("Port mapping request sent! (will timeout in {}ms)", timeout);

        // Wait for a response or timeout.
        thread::sleep(Duration::from_millis(timeout));
        match n.read_response_or_retry() {
            Err(e) => {
                println!("Failed to read NAT-PMP response: {:?}", e);
                if let Error::NATPMP_TRYAGAIN = e {
                    println!("Retry suggested by NAT-PMP. Trying again after a delay.");
                    thread::sleep(Duration::from_millis(timeout));
                } else {
                    return Err(anyhow!("Error reading NAT-PMP response: {:?}", e));
                }
            },
            Ok(response) => {
                match response {
                    Response::TCP(tr) => {
                        println!(
                            "Received TCP mapping response: Internal: {}, External: {}, Lifetime: {}s",
                            tr.private_port(),
                            tr.public_port(),
                            tr.lifetime().as_secs()
                        );
                        // Verify if the response matches the requested mapping, if applicable.
                        if !check
                            || (tr.private_port() == internal
                                && tr.public_port() == external
                                && tr.lifetime().as_secs() > 0)
                        {
                            return Ok(tr);
                        } else {
                            println!("Received port does not match requested parameters. Retrying...");
                        }
                    },
                    Response::UDP(ur) => {
                        println!(
                            "Received UDP mapping response (unexpected): Internal: {}, External: {}, Lifetime: {}s",
                            ur.private_port(),
                            ur.public_port(),
                            ur.lifetime().as_secs()
                        );
                    },
                    Response::Gateway(gr) => {
                        println!(
                            "Received public address response (unexpected): IP: {}, Epoch: {}",
                            gr.public_address(),
                            gr.epoch()
                        );
                    },
                }
            }
        };
        // Increase timeout for the next attempt.
        timeout *= 2;
    }
    bail!("Mapping failed after multiple attempts.");
}
