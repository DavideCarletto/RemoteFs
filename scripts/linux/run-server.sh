#!/bin/bash

# RemoteFS Server Control Script (Linux)
# Script per avviare/fermare il server Node.js del progetto RemoteFS

# Configurazione
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SERVER_DIR="$PROJECT_ROOT/server"
PID_FILE="/tmp/remote-fs-server.pid"
LOG_FILE="/tmp/remote-fs-server.log"
SERVER_PORT="${SERVER_PORT:-3000}"  # Usa variabile ambiente o default 3000

# Colori
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

# Funzioni di utility
print_header() {
    echo -e "${CYAN}🚀 RemoteFS Server Control${NC}"
    echo -e "${CYAN}═══════════════════════════${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

print_highlight() {
    echo -e "${PURPLE}🔗 $1${NC}"
}

# Funzioni di controllo server
get_server_pid() {
    if [ -f "$PID_FILE" ]; then
        cat "$PID_FILE" 2>/dev/null || echo ""
    else
        echo ""
    fi
}

is_server_running() {
    local pid="$1"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        return 0
    else
        return 1
    fi
}

get_running_node_processes() {
    # Cerca processi Node.js che eseguono server.ts o server.js
    pgrep -f "node.*server\.(ts|js)" 2>/dev/null || true
}

get_port_info() {
    local port="$1"
    if command -v lsof >/dev/null 2>&1; then
        lsof -i ":$port" 2>/dev/null
    elif command -v netstat >/dev/null 2>&1; then
        netstat -tuln 2>/dev/null | grep ":$port "
    elif command -v ss >/dev/null 2>&1; then
        ss -tuln 2>/dev/null | grep ":$port "
    else
        echo "Nessun tool disponibile per controllare la porta"
    fi
}

check_for_docker_server() {
    if command -v docker >/dev/null 2>&1; then
        # Controlla se c'è un container che usa la porta 3000
        local containers
        containers=$(docker ps --format "table {{.Names}}\t{{.Ports}}" | grep ":3000->" | cut -f1)
        if [ -n "$containers" ]; then
            echo "$containers"
            return 0
        fi
    fi
    return 1
}

is_port_in_use() {
    local port="$1"
    if command -v netstat >/dev/null 2>&1; then
        netstat -tuln 2>/dev/null | grep -q ":$port "
    elif command -v ss >/dev/null 2>&1; then
        ss -tuln 2>/dev/null | grep -q ":$port "
    else
        # Fallback usando lsof se disponibile
        if command -v lsof >/dev/null 2>&1; then
            lsof -i ":$port" >/dev/null 2>&1
        else
            return 1
        fi
    fi
}

check_dependencies() {
    print_info "Controllo dipendenze..."
    
    # Controlla Node.js
    if ! command -v node >/dev/null 2>&1; then
        print_error "Node.js non trovato!"
        print_info "Installa Node.js: https://nodejs.org/"
        return 1
    fi
    
    local node_version
    node_version=$(node --version)
    print_success "Node.js trovato: $node_version"
    
    # Controlla npm
    if ! command -v npm >/dev/null 2>&1; then
        print_error "npm non trovato!"
        return 1
    fi
    
    local npm_version
    npm_version=$(npm --version)
    print_success "npm trovato: $npm_version"
    
    # Controlla directory server
    if [ ! -d "$SERVER_DIR" ]; then
        print_error "Directory server non trovata: $SERVER_DIR"
        return 1
    fi
    
    # Controlla package.json
    if [ ! -f "$SERVER_DIR/package.json" ]; then
        print_error "package.json non trovato in: $SERVER_DIR"
        return 1
    fi
    
    # Controlla node_modules
    if [ ! -d "$SERVER_DIR/node_modules" ]; then
        print_warning "node_modules non trovato. Installazione dipendenze..."
        cd "$SERVER_DIR"
        if npm install; then
            print_success "Dipendenze installate"
        else
            print_error "Errore nell'installazione delle dipendenze"
            return 1
        fi
    fi
    
    return 0
}

start_server() {
    print_header
    
    # Controlla se il server è già in esecuzione
    local existing_processes
    existing_processes=$(get_running_node_processes)
    
    if [ -n "$existing_processes" ]; then
        print_error "Server già in esecuzione (PID: $existing_processes)"
        print_info "Usa: $0 stop per fermarlo"
        return 1
    fi
    
    # Controlla se la porta è in uso
    if is_port_in_use "$SERVER_PORT"; then
        print_error "Porta $SERVER_PORT già in uso!"
        
        # Controlla se è un container Docker
        local docker_containers
        docker_containers=$(check_for_docker_server)
        if [ $? -eq 0 ] && [ -n "$docker_containers" ]; then
            print_warning "Container Docker rilevato: $docker_containers"
            print_info "Opzioni:"
            print_info "  1. Ferma Docker: docker stop $docker_containers"
            print_info "  2. Usa porta diversa: SERVER_PORT=3001 $0 start"
            print_info "  3. Continua con Docker (raccomandato)"
        else
            print_info "Processo che usa la porta:"
            get_port_info "$SERVER_PORT"
            print_info "Controlla: netstat -tuln | grep :$SERVER_PORT"
        fi
        return 1
    fi
    
    # Controlla dipendenze
    if ! check_dependencies; then
        return 1
    fi
    
    print_info "🚀 Avvio server RemoteFS..."
    
    cd "$SERVER_DIR"
    
    # Controlla se esiste dist/src/server.js
    if [ ! -f "dist/src/server.js" ]; then
        print_info "📦 Compilazione TypeScript necessaria..."
        if npm run build; then
            print_success "Compilazione completata"
        else
            print_error "Errore nella compilazione TypeScript"
            return 1
        fi
    fi
    
    # Avvia il server in background
    print_info "Avvio server in background..."
    
    # Crea uno script temporaneo per l'avvio
    local start_script="/tmp/start-remote-fs-server.sh"
    cat > "$start_script" << 'EOF'
#!/bin/bash
cd "$1"
exec npm start
EOF
    chmod +x "$start_script"
    
    # Avvia in background con disown per detacharlo completamente
    "$start_script" "$SERVER_DIR" >"$LOG_FILE" 2>&1 &
    local server_pid=$!
    disown
    
    # Salva il PID
    echo "$server_pid" > "$PID_FILE"
    
    # Aspetta un po' per l'avvio
    sleep 3
    
    # Verifica che il processo sia ancora attivo
    if is_server_running "$server_pid"; then
        print_success "Server avviato con successo! PID: $server_pid"
        
        # Aspetta che il server sia pronto
        print_info "Verifica che il server sia pronto..."
        local wait_count=0
        while [ $wait_count -lt 15 ]; do
            if is_port_in_use "$SERVER_PORT"; then
                break
            fi
            sleep 1
            wait_count=$((wait_count + 1))
        done
        
        if is_port_in_use "$SERVER_PORT"; then
            print_success "🌐 Server pronto sulla porta $SERVER_PORT"
            print_highlight "URL: http://localhost:$SERVER_PORT"
            print_highlight "Health: http://localhost:$SERVER_PORT/health"
            print_info "📋 Log file: $LOG_FILE"
            print_info "🛑 Per fermare: $0 stop"
        else
            print_warning "Server avviato ma porta non ancora in ascolto"
            print_info "Controlla il log: tail -f $LOG_FILE"
        fi
    else
        print_error "Il server è terminato inaspettatamente"
        print_info "Controlla il log: cat $LOG_FILE"
        rm -f "$PID_FILE"
        return 1
    fi
}

stop_server() {
    print_header
    
    local pid
    pid=$(get_server_pid)
    local processes
    processes=$(get_running_node_processes)
    
    if [ -z "$pid" ] && [ -z "$processes" ]; then
        print_info "Nessun server RemoteFS in esecuzione"
        return 0
    fi
    
    # Prova prima con il PID dal file
    if [ -n "$pid" ] && is_server_running "$pid"; then
        print_info "🛑 Fermando server (PID: $pid)..."
        if kill "$pid" 2>/dev/null; then
            # Aspetta che il processo termini
            local count=0
            while is_server_running "$pid" && [ $count -lt 10 ]; do
                sleep 1
                count=$((count + 1))
            done
            
            if is_server_running "$pid"; then
                print_warning "Terminazione forzata necessaria"
                kill -9 "$pid" 2>/dev/null || true
            fi
            print_success "Server fermato"
        else
            print_error "Errore nel fermare il server"
        fi
    fi
    
    # Forza la terminazione di tutti i processi Node.js server rimanenti
    processes=$(get_running_node_processes)
    if [ -n "$processes" ]; then
        print_info "🔨 Terminazione forzata dei processi rimanenti..."
        echo "$processes" | while IFS= read -r proc_pid; do
            if kill -9 "$proc_pid" 2>/dev/null; then
                print_success "Processo $proc_pid terminato"
            else
                print_error "Errore nel terminare processo $proc_pid"
            fi
        done
    fi
    
    # Pulisci il PID file
    if [ -f "$PID_FILE" ]; then
        rm -f "$PID_FILE"
    fi
    
    print_success "🧹 Cleanup completato"
}

show_status() {
    print_header
    
    local pid
    pid=$(get_server_pid)
    local processes
    processes=$(get_running_node_processes)
    
    # Stato processi
    if [ -n "$processes" ]; then
        print_success "🔥 Server attivo:"
        echo "$processes" | while IFS= read -r proc_pid; do
            if [ -n "$proc_pid" ]; then
                local mem_usage
                mem_usage=$(ps -o rss= -p "$proc_pid" 2>/dev/null | tr -d ' ' || echo "N/A")
                if [ "$mem_usage" != "N/A" ]; then
                    mem_usage=$((mem_usage / 1024))
                    echo -e "   PID: $proc_pid | Memory: ${mem_usage}MB"
                else
                    echo -e "   PID: $proc_pid | Memory: N/A"
                fi
            fi
        done
    else
        print_error "🔥 Nessun server in esecuzione"
    fi
    
    # PID file
    if [ -n "$pid" ]; then
        echo -e "${CYAN}📄 PID file: $pid${NC}"
    else
        echo -e "${YELLOW}📄 PID file: Non trovato${NC}"
    fi
    
    # Porta
    if is_port_in_use "$SERVER_PORT"; then
        print_success "🌐 Porta $SERVER_PORT: In uso ✅"
        print_highlight "   URL: http://localhost:$SERVER_PORT"
    else
        print_error "🌐 Porta $SERVER_PORT: Libera ❌"
    fi
    
    # Log file
    if [ -f "$LOG_FILE" ]; then
        local log_size
        log_size=$(du -h "$LOG_FILE" 2>/dev/null | cut -f1 || echo "N/A")
        echo -e "${CYAN}📋 Log file: $LOG_FILE ($log_size)${NC}"
    else
        echo -e "${YELLOW}📋 Log file: Non trovato${NC}"
    fi
    
    # Test connettività
    if [ -n "$processes" ] && is_port_in_use "$SERVER_PORT"; then
        print_info "🔍 Test connettività..."
        if command -v curl >/dev/null 2>&1; then
            if curl -s -f "http://localhost:$SERVER_PORT/health" >/dev/null 2>&1; then
                print_success "   Health check: OK ✅"
            else
                print_error "   Health check: FAIL ❌"
            fi
        else
            print_warning "   curl non disponibile per test"
        fi
    fi
}

show_logs() {
    local lines="${1:-20}"
    
    print_header
    
    if [ -f "$LOG_FILE" ]; then
        echo -e "${CYAN}📋 Ultimi $lines righe del log:${NC}"
        echo -e "${CYAN}─────────────────────────────${NC}"
        tail -n "$lines" "$LOG_FILE"
    else
        print_error "File di log non trovato: $LOG_FILE"
    fi
}

restart_server() {
    print_header
    print_info "🔄 Riavvio server..."
    stop_server
    sleep 2
    start_server
}

install_deps() {
    print_header
    print_info "📦 Installazione dipendenze..."
    
    if [ ! -d "$SERVER_DIR" ]; then
        print_error "Directory server non trovata: $SERVER_DIR"
        return 1
    fi
    
    cd "$SERVER_DIR"
    
    if npm install; then
        print_success "Dipendenze installate con successo"
    else
        print_error "Errore nell'installazione delle dipendenze"
        return 1
    fi
}

show_help() {
    cat << EOF
RemoteFS Server Control Script (Linux)

Usage: $0 [action]

Actions:
  start     - Avvia il server Node.js
  stop      - Ferma il server Node.js
  restart   - Riavvia il server
  status    - Mostra stato del server e connettività
  logs [N]  - Mostra ultimi N log (default: 20)
  install   - Installa/aggiorna dipendenze npm
  help      - Mostra questo help

Examples:
  $0 start
  $0 status
  $0 logs 50
  $0 restart
  $0 install

Files:
  PID file: $PID_FILE
  Log file: $LOG_FILE
  Server dir: $SERVER_DIR
  Port: $SERVER_PORT

URLs:
  Server: http://localhost:$SERVER_PORT
  Health: http://localhost:$SERVER_PORT/health
EOF
}

# Main logic
ACTION="${1:-status}"

case "$ACTION" in
    "start")
        start_server
        ;;
    "stop")
        stop_server
        ;;
    "restart")
        restart_server
        ;;
    "status")
        show_status
        ;;
    "logs")
        show_logs "$2"
        ;;
    "install")
        install_deps
        ;;
    "help"|"--help"|"-h")
        show_help
        ;;
    *)
        print_error "Azione non riconosciuta: $ACTION"
        print_info "Usa 'help' per vedere le azioni disponibili"
        exit 1
        ;;
esac
