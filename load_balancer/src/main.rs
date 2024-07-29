use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock};
use std::sync::Arc;
use std::collections::HashMap;
use std::net::SocketAddr;
use rand::Rng;
use tokio::time::{self, Duration};
use std::io::{self, Write};

/// Les différents algorithmes de répartition de charge disponibles
#[derive(Debug, Clone, Copy)]
pub enum LoadBalancingAlgorithm {
    RoundRobin,
    Random,
    LeastConnection,
}

#[derive(Debug)]
/// Énumération pour les différentes erreurs possibles.
pub enum LoadBalancerError {
    IoError(std::io::Error),
    HealthCheckError(String),
}

impl From<std::io::Error> for LoadBalancerError {
    /// Implémentation de la conversion automatique d'une std::io::Error vers une LoadBalancerError.
    fn from(err: std::io::Error) -> Self {
        LoadBalancerError::IoError(err)
    }
}

/// La structure du load balancer
#[derive(Debug, Clone)]
pub struct LoadBalancer {
    servers: Arc<RwLock<HashMap<String, SocketAddr>>>,
    connections: Arc<RwLock<HashMap<SocketAddr, usize>>>,
    algorithm: LoadBalancingAlgorithm,
    current: Arc<Mutex<usize>>,
    connection_pool: Arc<Mutex<HashMap<SocketAddr, Vec<TcpStream>>>>,
}

impl LoadBalancer {
    // Constructeur pour initialiser le LoadBalancer avec un algorithme de répartition
    pub fn new(algorithm: LoadBalancingAlgorithm) -> Self {
        let server_addresses: HashMap<String, SocketAddr> = vec![
            ("web1".to_string(), "192.168.1.21:80".parse().unwrap()),
            ("web2".to_string(), "192.168.1.22:80".parse().unwrap()),
            ("web3".to_string(), "192.168.1.23:80".parse().unwrap()),
            ("web4".to_string(), "192.168.1.24:80".parse().unwrap()),
            ("web5".to_string(), "192.168.1.25:80".parse().unwrap()),
        ].into_iter().collect();

        let connections: HashMap<SocketAddr, usize> = server_addresses.values().cloned().map(|addr| (addr, 0)).collect();

        LoadBalancer {
            servers: Arc::new(RwLock::new(server_addresses)),
            connections: Arc::new(RwLock::new(connections)),
            algorithm,
            current: Arc::new(Mutex::new(0)),
            connection_pool: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn get_or_create_connection(&self, addr: SocketAddr) -> Result<TcpStream, LoadBalancerError> {
        let mut pool = self.connection_pool.lock().await;
        if let Some(connections) = pool.get_mut(&addr) {
            if let Some(conn) = connections.pop() {
                return Ok(conn);
            }
        }
        TcpStream::connect(addr).await.map_err(LoadBalancerError::IoError)
    }

    async fn return_connection(&self, addr: SocketAddr, conn: TcpStream) {
        let mut pool = self.connection_pool.lock().await;
        pool.entry(addr).or_insert_with(Vec::new).push(conn);
    }

    // Fonction pour implémenter l'algorithme Least Connection
    async fn least_connection(&self, servers: &HashMap<String, SocketAddr>) -> Option<SocketAddr> {
        let connections = self.connections.read().await;
        let min_connections = connections.values().min().cloned().unwrap_or(0);
        let servers_with_min = servers.values()
            .filter(|&addr| connections.get(addr).cloned().unwrap_or(0) == min_connections)
            .collect::<Vec<_>>();
        
        if servers_with_min.is_empty() {
            None
        } else {
            Some(*servers_with_min[rand::thread_rng().gen_range(0..servers_with_min.len())])
        }
    }

    /// Fonction asynchrone pour gérer la connexion d'un client.
    /// Choisit un serveur selon l'algorithme de répartition de charge et transfère les données entre le client et le serveur.
    /// Fonction pour gérer la connexion d'un client
    pub async fn handle_client(&self, mut client: TcpStream) -> Result<(), LoadBalancerError> {
        let server_addr = {
            let servers = self.servers.read().await;
            match self.algorithm {
                LoadBalancingAlgorithm::RoundRobin => self.round_robin(&servers).await,
                LoadBalancingAlgorithm::Random => self.random(&servers),
                LoadBalancingAlgorithm::LeastConnection => self.least_connection(&servers).await,
            }
        };
        
        if let Some(addr) = server_addr {
            println!("Forwarding request to server: {:?}", addr);
    
            {
                let mut connections = self.connections.write().await;
                *connections.entry(addr).or_insert(0) += 1;
            }

            let mut server = self.get_or_create_connection(addr).await?;

            let (mut client_reader, mut client_writer) = client.split();
            let (mut server_reader, mut server_writer) = server.split();
            
            let client_to_server = tokio::io::copy(&mut client_reader, &mut server_writer);
            let server_to_client = tokio::io::copy(&mut server_reader, &mut client_writer);

            if let Err(e) = tokio::try_join!(client_to_server, server_to_client) {
                eprintln!("Error during data copy: {:?}", e);
                return Err(LoadBalancerError::IoError(e));
            }

            self.return_connection(addr, server).await;

            {
                let mut connections = self.connections.write().await;
                if let Some(conn) = connections.get_mut(&addr) {
                    *conn = conn.saturating_sub(1);
                }
            }
        } else {
            eprintln!("No available servers");
        }
        
        Ok(())
    }

    /// Algorithme Round Robin pour sélectionner le prochain serveur dans la liste.
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

    /// Algorithme Random pour sélectionner un serveur aléatoirement dans la liste.
    fn random(&self, servers: &HashMap<String, SocketAddr>) -> Option<SocketAddr> {
        let mut rng = rand::thread_rng();
        let server_list: Vec<&SocketAddr> = servers.values().collect();
        let server_count = server_list.len();
        if server_count == 0 {
            return None;
        }
        Some(*server_list[rng.gen_range(0..server_count)])
    }

    /// Fonction asynchrone pour vérifier l'état de santé des serveurs.
    /// Supprime les serveurs non réactifs de la liste.
    pub async fn perform_health_check(&self) {
        let mut servers = self.servers.write().await;
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

    /// Fonction asynchrone pour démarrer un vérificateur d'état des serveurs à intervalles réguliers.
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

/// Fonction synchrone pour lire l'algorithme de répartition de charge choisi par l'utilisateur.
fn read_load_balancing_algorithm() -> LoadBalancingAlgorithm {
    println!("Choisissez l'algorithme de répartition de charge :");
    println!("1 : Round Robin");
    println!("2 : Random");
    println!("3 : Least Connection");

    loop {
        print!("Votre choix : ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        match input.trim() {
            "1" => return LoadBalancingAlgorithm::RoundRobin,
            "2" => return LoadBalancingAlgorithm::Random,
            "3" => return LoadBalancingAlgorithm::LeastConnection,
            _ => println!("Choix invalide, veuillez réessayer."),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), LoadBalancerError> {
    //! Fonction principale asynchrone.
    //! Lit l'algorithme de répartition choisi, initialise le load balancer, démarre la vérification de l'état des serveurs,
    //! et accepte les connexions clients.
    let algorithm = read_load_balancing_algorithm();

    let lb = LoadBalancer::new(algorithm);

    lb.start_health_check(30).await;

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

// Module de tests
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_round_robin() {
        let lb = LoadBalancer::new(LoadBalancingAlgorithm::RoundRobin);
        let servers = lb.servers.read().await;

        // Test de l'algorithme Round Robin
        let expected_servers = vec![
            "192.168.1.21:80".parse().unwrap(),
            "192.168.1.22:80".parse().unwrap(),
            "192.168.1.23:80".parse().unwrap(),
            "192.168.1.24:80".parse().unwrap(),
            "192.168.1.25:80".parse().unwrap(),
        ];

        for _ in 0..expected_servers.len() {
            assert!(expected_servers.contains(&lb.round_robin(&servers).await.unwrap()));
        }
    }

    #[tokio::test]
    async fn test_random() {
        let lb = LoadBalancer::new(LoadBalancingAlgorithm::Random);
        let servers = lb.servers.read().await;

        // Test de l'algorithme Random (les résultats peuvent varier)
        assert!(servers.values().any(|&addr| addr == lb.random(&servers).unwrap()));
    }

    #[tokio::test]
    async fn test_health_check() {
        let lb = LoadBalancer::new(LoadBalancingAlgorithm::RoundRobin);
        
        // Ajouter des serveurs fictifs
        {
            let mut servers = lb.servers.write().await;
            servers.insert("unreachable".to_string(), "10.255.255.1:80".parse().unwrap());
        }
        
        lb.perform_health_check().await;
        
        let servers = lb.servers.read().await;
        assert!(!servers.contains_key("unreachable"));
    }
}
