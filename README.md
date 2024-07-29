# README

#Projet fait depuis une distribution Debian 12 Bookworm
## Pré-requis pour faire tourner le projet

Prérequis
Rust (version 1.60 ou supérieure)
Cargo (gestionnaire de packages et outil de build pour Rust)
Docker
IDE (recommandée VisualStudio Code)

######################################################################

Installation de docker:

```sh
sudo apt-get update
sudo apt-get install \
    ca-certificates \
    curl \
    gnupg \
    lsb-release
sudo mkdir -p /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian \
  $(lsb_release -cs) stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
sudo apt-get update
sudo apt-get install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
sudo systemctl start docker
sudo systemctl enable docker
sudo usermod -aG docker $USER
newgrp docker

######################################################################
Installation de rust

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

######################################################################
Installation IDE recommandée  (VisualCode Studio)

sudo apt update
sudo apt install software-properties-common apt-transport-https wget
wget -q https://packages.microsoft.com/keys/microsoft.asc -O- | sudo apt-key add -
sudo add-apt-repository "deb [arch=amd64] https://packages.microsoft.com/repos/vscode stable main"
sudo apt update
sudo apt install code

######################################################################
Télécharger une version récentes de Docker-compose

sudo curl -L "https://github.com/docker/compose/releases/download/v2.21.0/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose

sudo chmod +x /usr/local/bin/docker-compose

docker-compose --version

######################################################################
Une fois les installations faîtes procéder aux tests pour voir si tout fonctionne:
Etape 1 : lancement instances serveurs web
~load_balancert/servers$ docker-compose up
(bloque le terminal pour visualiser les logs)

Etape 2 : lancement du load balancer 
~/load_balancer/load_balancer$ cargo run

Etape 3 : Vérification de la connectivitée - Effectuer la simulation des requêtes clients 
test individuel
curl http://localhost:8080 
test en masse
wrk -t12 -c400 -d30s http://localhost:8080
test unitaire
voir la rust doc

Options de wrk: (pour effectuer les tests de masse)                          
    -c, --connections <N>  Nombres de connexions à garder ouvertes  
    -d, --duration    <T>  durée du test  (en secondes)  
    -t, --threads     <N>  Nombres de thread à utiliser
######################################################################

Documentation du Load Balancer en Rust
Introduction
Ce projet est un load balancer en Rust capable de distribuer des requêtes HTTP entrantes vers un ensemble de serveurs web en utilisant deux algorithmes de répartition de charge : Round Robin et Random. Ce load balancer est conçu pour équilibrer la charge entre 5 serveurs web, garantissant ainsi une répartition efficace des requêtes et une meilleure gestion des ressources.
Fonctionnalités
Algorithme Round Robin : Distribue les requêtes de manière circulaire parmi les serveurs disponibles.
Algorithme Random : Sélectionne aléatoirement un serveur parmi les serveurs disponibles pour chaque requête.
Algorithme Least Connection: Distribue les requêtes vers le serveurs qui a le moins de connexion actives à un instant T

######################################################################
Round Robin

Dans notre projet nous avons 5 serveurs webs, le load balancer renvoie les requêtes vers ces mêmes serveurs webs ont s’appuyant sur un algorithme précis ici le Round Robin.

Les requêtes forwarder depuis le load balancer suivent cette ordre:

requête 1
vers —--------------->
web3_1

requête 2
 vers —--------------->
web5_1

requête 3
vers —--------------->
web4_1

requête 4
vers —--------------->
web2_1

requête 5
vers —--------------->
web1_1

requête 6
vers —--------------->
web3_1

Retour de simulation côté serveur web

######################################################################
Health Check 

Quand on fait le cargo run pour lancer le load balancer, on a une entrée user pour sélectionner l’algo a utilisé (ici Random) puis en dessous on a un premier retour healthcheck de nos serveurs web actifs:

Le premier retour est issu de notre fonction performance_health_check dans le main.rs

Puis on a un check périodique l’état des serveurs webs (issu de la fonction start_health_check dans le main.rs):

######################################################################
 Random

Dans notre projet nous avons 5 serveurs webs, le load balancer renvoie les requêtes vers ces mêmes serveurs webs ont s’appuyant sur un algorithme précis ici l’algo Random.

Les requêtes forwarder depuis le load balancer arrivent sur le serveur web aléatoirement:

requête numéro  X
vers —--------------->
webX_1 (X à remplacer pour 1, 2, 3, 4 ou 5 en fonction du serveur sélectionnées aléatoirement


Retour de simulation côté serveur web

Sur cette capture d’écran on peut voir que les serveurs cible n’ont pas un ordre périodique

######################################################################
Contributions
Les contributions sont les bienvenues ! Pour contribuer, veuillez suivre les étapes suivantes :
Forker le dépôt.
Créer une branche pour votre fonctionnalité ou correctif.
Soumettre une pull request avec une description détaillée de vos modifications.
License
Ce projet est sous la licence MIT. Voir le fichier LICENSE pour plus de détails.
Support
Pour toute question ou problème, veuillez ouvrir une issue sur le dépôt GitHub du projet.


