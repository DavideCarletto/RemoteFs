#!/usr/bin/env pwsh
# Script per gestire il server RemoteFS Docker

param(
    [string]$Action = "start"
)

$IMAGE_NAME = "remotefs-server"
$CONTAINER_NAME = "remotefs-server"
$PORT = 3000
$PROJECT_ROOT = Split-Path -Path (Split-Path -Path $PSScriptRoot -Parent) -Parent
$SERVER_DIR = Join-Path $PROJECT_ROOT "server"

switch ($Action.ToLower()) {
    "build" {
        Write-Host "🔨 Building RemoteFS server image..." -ForegroundColor Yellow
        Write-Host "📁 Project root: $PROJECT_ROOT" -ForegroundColor Cyan
        Write-Host "📁 Server directory: $SERVER_DIR" -ForegroundColor Cyan
        
        if (Test-Path $SERVER_DIR) {
            docker build -t $IMAGE_NAME $SERVER_DIR
        } else {
            Write-Host "❌ Server directory not found: $SERVER_DIR" -ForegroundColor Red
            exit 1
        }
    }
    "start" {
        Write-Host "🚀 Starting RemoteFS server..." -ForegroundColor Green
        # Ferma il container se esiste già
        docker stop $CONTAINER_NAME 2>$null
        docker rm $CONTAINER_NAME 2>$null
        
        # Avvia il nuovo container
        docker run -d `
            --name $CONTAINER_NAME `
            -p ${PORT}:3000 `
            -v remotefs-data:/app/data `
            $IMAGE_NAME
        
        Write-Host "✅ Server started on http://localhost:$PORT" -ForegroundColor Green
        Write-Host "📊 Health check: http://localhost:$PORT/health" -ForegroundColor Cyan
    }
    "stop" {
        Write-Host "🛑 Stopping RemoteFS server..." -ForegroundColor Red
        docker stop $CONTAINER_NAME
        docker rm $CONTAINER_NAME
    }
    "restart" {
        Write-Host "🔄 Restarting RemoteFS server..." -ForegroundColor Yellow
        & $MyInvocation.MyCommand.Path -Action stop
        & $MyInvocation.MyCommand.Path -Action start
    }
    "logs" {
        Write-Host "📋 Server logs:" -ForegroundColor Cyan
        docker logs -f $CONTAINER_NAME
    }
    "status" {
        Write-Host "📊 Server status:" -ForegroundColor Cyan
        docker ps --filter "name=$CONTAINER_NAME"
        Write-Host ""
        try {
            $response = Invoke-RestMethod -Uri "http://localhost:$PORT/health" -TimeoutSec 5
            Write-Host "✅ Health check: $($response.status)" -ForegroundColor Green
        } catch {
            Write-Host "❌ Health check failed: Server not responding" -ForegroundColor Red
        }
    }
    "clean" {
        Write-Host "🧹 Cleaning up..." -ForegroundColor Yellow
        docker stop $CONTAINER_NAME 2>$null
        docker rm $CONTAINER_NAME 2>$null
        docker rmi $IMAGE_NAME 2>$null
        docker volume rm remotefs-data 2>$null
    }
    default {
        Write-Host @"
RemoteFS Server Management Script

Usage: .\run-server.ps1 [action]

Actions:
  build     - Build the server Docker image
  start     - Start the server (default)
  stop      - Stop the server
  restart   - Restart the server
  logs      - Show server logs
  status    - Show server status and health
  clean     - Stop and remove all containers, images, and volumes

Examples:
  .\run-server.ps1 build
  .\run-server.ps1 start
  .\run-server.ps1 logs
"@ -ForegroundColor White
    }
}
