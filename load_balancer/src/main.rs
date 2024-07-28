use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use std::sync::Arc;
use std::collections::HashMap;
use std::net::SocketAddr;
use rand::Rng;
use tokio::time::{self, Duration};
use std::io::{self, Write};

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
    // Constructeur pour initialiser le LoadBalancer avec un algorithme de répartition
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

    // Fonction pour gérer la connexion d'un client
    pub async fn handle_client(&self, mut client: TcpStream) -> Result<(), LoadBalancerError> {
        let servers = self.servers.lock().await;
        let server_addr = match self.algorithm {
            LoadBalancingAlgorithm::RoundRobin => self.round_robin(&servers).await,
            LoadBalancingAlgorithm::Random => self.random(&servers),
        };
        
        if let Some(addr) = server_addr {
            println!("Forwarding request to server: {:?}", addr);
            match TcpStream::connect(addr).await {
                Ok(mut server) => {
                    let (mut client_reader, mut client_writer) = client.split();
                    let (mut server_reader, mut server_writer) = server.split();
                    
                    let client_to_server = tokio::io::copy(&mut client_reader, &mut server_writer);
                    let server_to_client = tokio::io::copy(&mut server_reader, &mut client_writer);
                    
                    if let Err(e) = tokio::try_join!(client_to_server, server_to_client) {
                        eprintln!("Error during data copy: {:?}", e);
                        return Err(LoadBalancerError::IoError(e));
                    }
                },
                Err(e) => {
                    eprintln!("Failed to connect to server: {:?}", e);
                    return Err(LoadBalancerError::IoError(e));
                }
            }
        } else {
            eprintln!("No available servers");
        }
        
        Ok(())
    }

    // Fonction pour implémenter l'algorithme Round Robin
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

    // Fonction pour implémenter l'algorithme aléatoire
    fn random(&self, servers: &HashMap<String, SocketAddr>) -> Option<SocketAddr> {
        let mut rng = rand::thread_rng();
        let server_list: Vec<&SocketAddr> = servers.values().collect();
        let server_count = server_list.len();
        if server_count == 0 {
            return None;
        }
        Some(*server_list[rng.gen_range(0..server_count)])
    }

    // Fonction pour vérifier l'état des serveurs
    pub async fn perform_health_check(&self) {
        let mut servers = self.servers.lock().await;
        let server_list: Vec<(String, SocketAddr)> = servers.iter().map(|(name, &addr)| (name.clone(), addr)).collect();

        for (name, addr) in server_list {
            match TcpStream::connect(addr).await {
                Ok(_) => {
                    println!("Server {} is healthy", name);
                }
                Err(_) => {
                    println!("Server {} is not responding, removing from list", name);
                    servers.remove(&name);
                }
            }
        }
    }

    // Fonction pour démarrer la vérification de l'état des serveurs à intervalles réguliers
    pub async fn start_health_check(&self, interval_secs: u64) {
        let interval = Duration::from_secs(interval_secs);
        let lb = self.clone();
        tokio::spawn(async move {
            let mut interval_timer = time::interval(interval);
            loop {
                interval_timer.tick().await;
                lb.perform_health_check().await;
            }
        });
    }
}

// Fonction pour lire l'algorithme de répartition choisi par l'utilisateur
fn read_load_balancing_algorithm() -> LoadBalancingAlgorithm {
    println!("Choisissez l'algorithme de répartition de charge :");
    println!("1 : Round Robin");
    println!("2 : Random");
    
    loop {
        print!("Votre choix : ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        
        match input.trim() {
            "1" => return LoadBalancingAlgorithm::RoundRobin,
            "2" => return LoadBalancingAlgorithm::Random,
            _ => println!("Choix invalide, veuillez réessayer."),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), LoadBalancerError> {
    // Lire l'algorithme de répartition de charge choisi par l'utilisateur
    let algorithm = read_load_balancing_algorithm();
    
    // Initialisation du load balancer avec l'algorithme choisi
    let lb = LoadBalancer::new(algorithm);
    
    // Démarrage de la vérification de l'état des serveurs toutes les 30 secondes
    lb.start_health_check(30).await; 
    
    // Lancement du listener pour accepter les connexions clients
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    // Boucle principale pour accepter les connexions clients
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
