#!/bin/bash

# Clean up Docker resources
echo "Stopping and removing all Docker containers..."
docker-compose down --rmi all --volumes --remove-orphans

echo "Pruning Docker system..."
docker system prune -af --volumes

# Rebuild and run the containers
echo "Building and running Docker containers..."
docker-compose up --build
