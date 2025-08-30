# Configurazione
PID_FILE="/tmp/remote-fs-client.pid"
LOG_FILE="/tmp/remote-fs-client.log"
DEFAULT_MOUNT_POINT="/tmp/remote-fs"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CLIENT_DIR="$PROJECT_ROOT/client"

# Colori
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Funzioni di utility
print_status() {
    echo -e "${CYAN}📊 Stato RemoteFS:${NC}"
    echo -e "${CYAN}─────────────────${NC}"
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

# Funzioni principali
get_pid_from_file() {
    if [ -f "$PID_FILE" ]; then
        cat "$PID_FILE" 2>/dev/null || echo ""
    else
        echo ""
    fi
}

is_process_running() {
    local pid="$1"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        return 0
    else
        return 1
    fi
}

get_running_processes() {
    # Cerca processi remote_fs specifici, escludendo grep stesso
    pgrep -f "target/.*remote_fs" 2>/dev/null || true
}

get_mount_point() {
    mount | grep "remote-fs" | awk '{print $3}' | head -n1
}

is_mounted() {
    [ -n "$(get_mount_point)" ]
}

start_remotefs() {
    local existing_processes
    existing_processes=$(get_running_processes)
    
    if [ -n "$existing_processes" ]; then
        print_error "RemoteFS già in esecuzione (PID: $existing_processes)"
        return 1
    fi

    print_info "🚀 Avvio RemoteFS daemon..."
    
    if [ ! -d "$CLIENT_DIR" ]; then
        print_error "Directory client non trovata: $CLIENT_DIR"
        print_info "Assicurati di essere nella directory root del progetto RemoteFS"
        return 1
    fi

    cd "$CLIENT_DIR"
    
    # Carica l'ambiente Rust se disponibile
    if [ -f "$HOME/.cargo/env" ]; then
        source "$HOME/.cargo/env"
    fi
    
    # Controlla che cargo sia disponibile
    if ! command -v cargo >/dev/null 2>&1; then
        print_error "Cargo non trovato. Installa Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        return 1
    fi
    
    # Avvia in background con nohup
    print_info "Avvio del daemon in corso..."
    
    # Avvia il daemon in background e cattura l'output
    nohup cargo run --release >/tmp/remote-fs-startup.log 2>&1 &
    local cargo_pid=$!
    
    print_info "Processo cargo avviato (PID: $cargo_pid)"
    
    # Aspetta che il PID file venga creato o che appaia un processo daemon
    local wait_count=0
    local max_wait=30  # Aspetta max 30 secondi
    
    while [ $wait_count -lt $max_wait ]; do
        # Controlla se il PID file è stato creato
        if [ -f "$PID_FILE" ]; then
            print_info "PID file trovato dopo ${wait_count}s"
            break
        fi
        
        # Controlla se ci sono processi daemon attivi (anche senza PID file)
        local active_processes
        active_processes=$(get_running_processes)
        if [ -n "$active_processes" ]; then
            print_info "Processo daemon rilevato dopo ${wait_count}s"
            # Aspetta ancora un po' per il PID file
            sleep 2
            if [ -f "$PID_FILE" ]; then
                break
            fi
            # Se non c'è PID file ma c'è processo, va bene comunque
            print_warning "Daemon attivo ma PID file non ancora creato"
            break
        fi
        
        sleep 1
        wait_count=$((wait_count + 1))
        
        # Mostra progresso ogni 5 secondi
        if [ $((wait_count % 5)) -eq 0 ]; then
            print_info "Attendendo avvio daemon... (${wait_count}/${max_wait}s)"
        fi
    done
    
    # Verifica finale dello stato
    local final_processes
    final_processes=$(get_running_processes)
    
    if [ -n "$final_processes" ]; then
        # C'è almeno un processo attivo
        local pid
        pid=$(get_pid_from_file)
        
        if [ -n "$pid" ] && is_process_running "$pid"; then
            print_success "✅ Daemon avviato con successo! PID: $pid"
        else
            # Processo attivo ma senza PID file valido
            print_success "✅ Daemon avviato con successo! PID: $final_processes"
            print_warning "⚠️  PID file non disponibile, ma processo attivo"
        fi
        
        # Controlla mount point
        sleep 2
        if is_mounted; then
            print_success "📁 Filesystem montato su: $(get_mount_point)"
        else
            print_warning "⚠️  Daemon attivo, mount in corso... (controlla con 'status')"
        fi
        
        return 0
    else
        print_error "❌ Daemon non avviato correttamente"
        print_info "📋 Log di avvio:"
        if [ -f "/tmp/remote-fs-startup.log" ]; then
            tail -20 /tmp/remote-fs-startup.log
        fi
        print_info "📋 Log client:"
        if [ -f "/tmp/remote-fs-client.log" ]; then
            tail -10 /tmp/remote-fs-client.log
        fi
        return 1
    fi
}

stop_remotefs() {
    local pid
    pid=$(get_pid_from_file)
    local processes
    processes=$(get_running_processes)
    
    if [ -z "$pid" ] && [ -z "$processes" ]; then
        print_info "Nessun processo RemoteFS trovato"
        return 0
    fi

    # Prova prima con il PID dal file
    if [ -n "$pid" ] && is_process_running "$pid"; then
        print_info "🛑 Fermando RemoteFS (PID: $pid)..."
        if kill "$pid" 2>/dev/null; then
            # Aspetta che il processo termini
            local count=0
            while is_process_running "$pid" && [ $count -lt 10 ]; do
                sleep 1
                count=$((count + 1))
            done
            
            if is_process_running "$pid"; then
                print_warning "Terminazione forzata necessaria"
                kill -9 "$pid" 2>/dev/null || true
            fi
            print_success "Processo fermato"
        else
            print_error "Errore nel fermare il processo"
        fi
    fi

    # Forza la terminazione di tutti i processi remote_fs rimanenti
    processes=$(get_running_processes)
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

    # Controlla se il mount point è ancora attivo
    if is_mounted; then
        local mount_point
        mount_point=$(get_mount_point)
        print_warning "Mount point ancora attivo: $mount_point"
        print_info "Usa: fusermount -u $mount_point per smontare manualmente"
    fi

    print_success "🧹 Cleanup completato"
}

show_status() {
    print_status
    
    local pid
    pid=$(get_pid_from_file)
    local processes
    processes=$(get_running_processes)
    
    if [ -n "$processes" ]; then
        print_success "Processi attivi:"
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
        print_error "Nessun processo RemoteFS trovato"
    fi
    
    if [ -n "$pid" ]; then
        echo -e "${CYAN}📄 PID file: $pid${NC}"
    else
        echo -e "${YELLOW}📄 PID file: Non trovato${NC}"
    fi
    
    # Controlla mount point
    if is_mounted; then
        local mount_point
        mount_point=$(get_mount_point)
        print_success "💾 Mount point attivo: $mount_point"
        
        # Testa l'accessibilità
        if [ -d "$mount_point" ] && ls "$mount_point" >/dev/null 2>&1; then
            print_success "   Accessibile ✅"
        else
            print_error "   Non accessibile ❌"
        fi
    else
        print_error "💾 Nessun mount point trovato"
    fi
    
    # Info sui log
    if [ -f "$LOG_FILE" ]; then
        local log_size
        log_size=$(du -h "$LOG_FILE" 2>/dev/null | cut -f1 || echo "N/A")
        echo -e "${CYAN}📋 Log file: $LOG_FILE ($log_size)${NC}"
    else
        echo -e "${YELLOW}📋 Log file: Non trovato${NC}"
    fi
}

show_logs() {
    local lines="${1:-20}"
    
    if [ -f "$LOG_FILE" ]; then
        echo -e "${CYAN}📋 Ultimi $lines righe del log:${NC}"
        echo -e "${CYAN}─────────────────────────────${NC}"
        tail -n "$lines" "$LOG_FILE"
    else
        print_error "File di log non trovato: $LOG_FILE"
    fi
}

restart_remotefs() {
    print_info "🔄 Riavvio RemoteFS..."
    stop_remotefs
    sleep 2
    start_remotefs
}

run_tests() {
    print_info "🧪 Esecuzione test RemoteFS Linux..."
    
    # Percorso della directory tests
    local tests_dir="$PROJECT_ROOT/tests"
    local test_script="$tests_dir/test_fuse_backend.sh"
    
    if [ ! -d "$tests_dir" ]; then
        print_error "Directory tests non trovata: $tests_dir"
        return 1
    fi
    
    if [ ! -f "$test_script" ]; then
        print_error "Script di test non trovato: $test_script"
        return 1
    fi
    
    # Verifica che il daemon sia attivo
    local processes
    processes=$(get_running_processes)
    if [ -z "$processes" ]; then
        print_warning "⚠️  Daemon non in esecuzione. Avvialo prima con: $0 start"
        print_info "Vuoi avviare il daemon automaticamente? (y/N)"
        read -r response
        if [[ "$response" =~ ^[Yy]$ ]]; then
            print_info "Avvio daemon..."
            start_remotefs
            if [ $? -ne 0 ]; then
                print_error "Impossibile avviare il daemon"
                return 1
            fi
            sleep 3
        else
            return 1
        fi
    fi
    
    # Prepara l'ambiente di test
    print_info "Preparazione ambiente di test..."
    
    # Crea il file di log con permessi corretti
    local test_log="/tmp/remotefs_test.log"
    touch "$test_log" 2>/dev/null || {
        print_warning "Permessi limitati per /tmp, uso directory alternativa"
        test_log="$HOME/remotefs_test.log"
        touch "$test_log"
    }
    
    # Rendi eseguibile lo script di test
    chmod +x "$test_script"
    
    print_info "Avvio script di test: $test_script"
    print_info "Log di test: $test_log"
    print_info "─────────────────────────────────────────"
    
    # Salva la directory corrente
    local original_dir="$(pwd)"
    
    # Esegui lo script di test con variabili ambiente
    cd "$tests_dir"
    export LOG_FILE="$test_log"
    export SKIP_SUDO="1"  # Salta operazioni sudo
    
    local test_result=0
    if bash "test_fuse_backend.sh"; then
        print_success "✅ Test completati con successo!"
        print_info "📋 Log completo: $test_log"
    else
        test_result=1
        print_error "❌ Test falliti"
        print_info "📋 Controlla il log: $test_log"
        if [ -f "$test_log" ]; then
            print_info "Ultimi errori:"
            tail -10 "$test_log" | while IFS= read -r line; do
                echo "   $line"
            done
        fi
    fi
    
    # Torna sempre alla directory originale
    cd "$original_dir"
    
    return $test_result
}

show_help() {
    cat << EOF
RemoteFS Client Control Script (Linux)

Usage: $0 [action]

Actions:
  start     - Avvia il daemon RemoteFS
  stop      - Ferma il daemon RemoteFS  
  restart   - Riavvia il daemon
  status    - Mostra stato dei processi e mount
  logs [N]  - Mostra ultimi N log (default: 20)
  test      - Esegue i test del filesystem FUSE
  help      - Mostra questo help

Examples:
  $0 start
  $0 status  
  $0 logs 50
  $0 test
  $0 stop

Files:
  PID file: $PID_FILE
  Log file: $LOG_FILE
  Mount point: $DEFAULT_MOUNT_POINT (default)
EOF
}

# Main logic
ACTION="${1:-status}"

case "$ACTION" in
    "start")
        start_remotefs
        ;;
    "stop")
        stop_remotefs
        ;;
    "restart")
        restart_remotefs
        ;;
    "status")
        show_status
        ;;
    "logs")
        show_logs "$2"
        ;;
    "test")
        run_tests
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
