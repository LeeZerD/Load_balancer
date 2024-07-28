use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use std::sync::Arc;
use std::collections::HashMap;
use std::net::SocketAddr;
use rand::Rng;

#[derive(Debug, Clone, Copy)]
pub enum LoadBalancingAlgorithm {
    RoundRobin,
    Random,
}

#[derive(Debug)]
pub enum LoadBalancerError {
    IoError(std::io::Error),
    HealthCheckError(String),
}

impl From<std::io::Error> for LoadBalancerError {
    fn from(err: std::io::Error) -> Self {
        LoadBalancerError::IoError(err)
    }
}

#[derive(Debug, Clone)]
pub struct LoadBalancer {
    servers: Arc<Mutex<HashMap<String, SocketAddr>>>,
    algorithm: LoadBalancingAlgorithm,
    current: Arc<Mutex<usize>>,
}

impl LoadBalancer {
    pub fn new(algorithm: LoadBalancingAlgorithm) -> Self {
        let server_addresses = vec![
            ("web1".to_string(), "192.168.1.21:80".parse().unwrap()),
            ("web2".to_string(), "192.168.1.22:80".parse().unwrap()),
            ("web3".to_string(), "192.168.1.23:80".parse().unwrap()),
            ("web4".to_string(), "192.168.1.24:80".parse().unwrap()),
            ("web5".to_string(), "192.168.1.25:80".parse().unwrap()),
        ].into_iter().collect();

        LoadBalancer {
            servers: Arc::new(Mutex::new(server_addresses)),
            algorithm,
            current: Arc::new(Mutex::new(0)),
        }
    }

    pub async fn handle_client(&self, mut client: TcpStream) -> Result<(), LoadBalancerError> {
        let servers = self.servers.lock().await;
        let server_addr = match self.algorithm {
            LoadBalancingAlgorithm::RoundRobin => self.round_robin(&servers).await,
            LoadBalancingAlgorithm::Random => self.random(&servers),
        };
        
        if let Some(addr) = server_addr {
            println!("Forwarding request to server: {:?}", addr);
            let mut server = TcpStream::connect(addr).await?;
            let (mut client_reader, mut client_writer) = client.split();
            let (mut server_reader, mut server_writer) = server.split();
            
            let client_to_server = tokio::io::copy(&mut client_reader, &mut server_writer);
            let server_to_client = tokio::io::copy(&mut server_reader, &mut client_writer);
            
            tokio::try_join!(client_to_server, server_to_client)
                .map_err(|e| {
                    eprintln!("Error during data copy: {:?}", e);
                    LoadBalancerError::IoError(e)
                })?;
        } else {
            eprintln!("No available servers");
        }
        
        Ok(())
    }
    

    async fn round_robin(&self, servers: &HashMap<String, SocketAddr>) -> Option<SocketAddr> {
        let mut current = self.current.lock().await;
        let server_count = servers.len();
        if server_count == 0 {
            return None;
        }
        let server_list: Vec<&SocketAddr> = servers.values().collect();
        let addr = server_list[*current];
        *current = (*current + 1) % server_count;
        Some(*addr)
    }

    fn random(&self, servers: &HashMap<String, SocketAddr>) -> Option<SocketAddr> {
        let mut rng = rand::thread_rng();
        let server_list: Vec<&SocketAddr> = servers.values().collect();
        let server_count = server_list.len();
        if server_count == 0 {
            return None;
        }
        Some(*server_list[rng.gen_range(0..server_count)])
    }

    pub async fn perform_health_check(&self) {
        // Placeholder for health check logic
    }

    
}

#[tokio::main]
async fn main() -> Result<(), LoadBalancerError> {
    println!("Starting load balancer...");
    let lb = LoadBalancer::new(LoadBalancingAlgorithm::RoundRobin);
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (client, _) = listener.accept().await?;
        let lb_clone = lb.clone();
        tokio::spawn(async move {
            if let Err(e) = lb_clone.handle_client(client).await {
                eprintln!("Error handling client: {:?}", e);
            }
        });
    }
}
