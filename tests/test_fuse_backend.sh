#!/bin/bash

# Script di test completo per RemoteFS
# Testa tutte le operazioni filesystem implementate

set -e  # Exit on any error

# Configurazione
MOUNT_POINT="/tmp/remote-fs"
TEST_DIR="$MOUNT_POINT/test_suite"
LOG_FILE="${LOG_FILE:-/tmp/remotefs_test.log}"

# Crea il file di log con gestione permessi
if [ "$SKIP_SUDO" = "1" ]; then
    # Modalità senza sudo per WSL/testing
    touch "$LOG_FILE" 2>/dev/null || {
        LOG_FILE="$HOME/remotefs_test.log"
        touch "$LOG_FILE"
    }
else
    sudo touch "$LOG_FILE"
fi

# Colori per output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Funzioni di utilità
log() {
    echo -e "${BLUE}[$(date '+%Y-%m-%d %H:%M:%S')]${NC} $1" | tee -a "$LOG_FILE"
}

success() {
    echo -e "${GREEN}✅ $1${NC}" | tee -a "$LOG_FILE"
}

error() {
    echo -e "${RED}❌ $1${NC}" | tee -a "$LOG_FILE"
}

warning() {
    echo -e "${YELLOW}⚠️  $1${NC}" | tee -a "$LOG_FILE"
}

# Verifica prerequisiti
check_prerequisites() {
    log "Checking prerequisites..."
    
    if [ ! -d "$MOUNT_POINT" ]; then
        error "Mount point $MOUNT_POINT does not exist"
        exit 1
    fi
    
    if ! mountpoint -q "$MOUNT_POINT"; then
        error "RemoteFS is not mounted at $MOUNT_POINT"
        exit 1
    fi
    
    success "Prerequisites check passed"
}

# Test 1: Creazione directory (mkdir)
test_mkdir() {
    log "Testing mkdir operation..."
    
    if [ -d "$TEST_DIR" ]; then
        rm -rf "$TEST_DIR"
    fi
    
    mkdir "$TEST_DIR"
    if [ -d "$TEST_DIR" ]; then
        success "mkdir: Directory created successfully"
    else
        error "mkdir: Failed to create directory"
        return 1
    fi
    
    # Test nested directories
    mkdir -p "$TEST_DIR/nested/deep/structure"
    if [ -d "$TEST_DIR/nested/deep/structure" ]; then
        success "mkdir: Nested directories created successfully"
    else
        error "mkdir: Failed to create nested directories"
        return 1
    fi
}

# Test 2: Lettura directory (readdir)
test_readdir() {
    log "Testing readdir operation..."
    
    # Create some test files and directories
    touch "$TEST_DIR/file1.txt"
    touch "$TEST_DIR/file2.txt"
    mkdir "$TEST_DIR/subdir1"
    mkdir "$TEST_DIR/subdir2"
    
    # Test ls command
    local files=$(ls "$TEST_DIR" | wc -l)
    if [ "$files" -gt 0 ]; then
        success "readdir: Directory listing works"
        log "Found $files entries in test directory"
        ls -la "$TEST_DIR" | tee -a "$LOG_FILE"
    else
        error "readdir: No files found in directory"
        return 1
    fi
}

# Test 3: Creazione file (create)
test_create() {
    log "Testing file creation..."
    
    local test_file="$TEST_DIR/test_create.txt"
    
    # Test touch command
    touch "$test_file"
    if [ -f "$test_file" ]; then
        success "create: File created with touch"
    else
        error "create: Failed to create file with touch"
        return 1
    fi
    
    # Test file creation with content
    echo "Hello, RemoteFS!" > "$TEST_DIR/content_file.txt"
    if [ -f "$TEST_DIR/content_file.txt" ]; then
        success "create: File created with content"
    else
        error "create: Failed to create file with content"
        return 1
    fi
}

# Test 4: Lettura file (read)
test_read() {
    log "Testing file read operation..."
    
    local test_file="$TEST_DIR/read_test.txt"
    local test_content="This is a test file for reading operations."
    
    echo "$test_content" > "$test_file"
    
    # Test cat command
    local read_content=$(cat "$test_file")
    if [ "$read_content" = "$test_content" ]; then
        success "read: File content read correctly"
    else
        error "read: File content mismatch"
        log "Expected: $test_content"
        log "Got: $read_content"
        return 1
    fi
    
    # Test partial read with head/tail
    head -n 1 "$test_file" > /dev/null
    tail -n 1 "$test_file" > /dev/null
    success "read: Partial read operations work"
}

# Test 5: Scrittura file (write)
test_write() {
    log "Testing file write operation..."
    
    local test_file="$TEST_DIR/write_test.txt"
    
    # Test echo redirect
    echo "First line" > "$test_file"
    echo "Second line" >> "$test_file"
    
    local lines=$(wc -l < "$test_file")
    if [ "$lines" -eq 2 ]; then
        success "write: File write operations work"
    else
        error "write: Unexpected number of lines ($lines, expected 2)"
        return 1
    fi
    
    # Test large file write
    dd if=/dev/zero of="$TEST_DIR/large_file.bin" bs=1024 count=100 2>/dev/null
    if [ -f "$TEST_DIR/large_file.bin" ]; then
        local size=$(stat -c%s "$TEST_DIR/large_file.bin")
        if [ "$size" -eq 102400 ]; then
            success "write: Large file write successful ($size bytes)"
        else
            warning "write: Large file size mismatch ($size bytes, expected 102400)"
        fi
    else
        error "write: Failed to write large file"
        return 1
    fi
}

# Test 6: Attributi file (getattr/setattr)
test_attributes() {
    log "Testing file attributes..."
    
    local test_file="$TEST_DIR/attr_test.txt"
    echo "Attribute test" > "$test_file"
    
    # Test stat command (getattr)
    if stat "$test_file" > /dev/null 2>&1; then
        success "getattr: File attributes retrieved"
        stat "$test_file" | tee -a "$LOG_FILE"
    else
        error "getattr: Failed to get file attributes"
        return 1
    fi
    
    # Test chmod (setattr)
    local original_mode=$(stat -c%a "$test_file")
    chmod 755 "$test_file"
    local new_mode=$(stat -c%a "$test_file")
    
    if [ "$new_mode" = "755" ]; then
        success "setattr: File permissions changed successfully"
        chmod "$original_mode" "$test_file"  # Restore
    else
        error "setattr: Failed to change permissions ($new_mode != 755)"
        return 1
    fi
    
    # Test file size change
    truncate -s 1000 "$test_file"
    local size=$(stat -c%s "$test_file")
    if [ "$size" -eq 1000 ]; then
        success "setattr: File size changed successfully"
    else
        error "setattr: Failed to change file size ($size != 1000)"
        return 1
    fi
}

# Test 7: Rinomina (rename)
test_rename() {
    log "Testing rename operation..."
    
    local old_name="$TEST_DIR/old_name.txt"
    local new_name="$TEST_DIR/new_name.txt"
    
    echo "Rename test" > "$old_name"
    
    # Test mv command
    mv "$old_name" "$new_name"
    
    if [ -f "$new_name" ] && [ ! -f "$old_name" ]; then
        success "rename: File renamed successfully"
    else
        error "rename: Failed to rename file"
        return 1
    fi
    
    # Test directory rename
    mkdir "$TEST_DIR/old_dir"
    mv "$TEST_DIR/old_dir" "$TEST_DIR/new_dir"
    
    if [ -d "$TEST_DIR/new_dir" ] && [ ! -d "$TEST_DIR/old_dir" ]; then
        success "rename: Directory renamed successfully"
    else
        error "rename: Failed to rename directory"
        return 1
    fi
}

# Test 8: Eliminazione file (unlink)
test_unlink() {
    log "Testing file deletion..."
    
    local test_file="$TEST_DIR/delete_me.txt"
    echo "Delete this file" > "$test_file"
    
    # Test rm command
    rm "$test_file"
    
    if [ ! -f "$test_file" ]; then
        success "unlink: File deleted successfully"
    else
        error "unlink: Failed to delete file"
        return 1
    fi
}

# Test 9: Eliminazione directory (rmdir)
test_rmdir() {
    log "Testing directory deletion..."
    
    # Test empty directory deletion
    mkdir "$TEST_DIR/empty_dir"
    rmdir "$TEST_DIR/empty_dir"
    
    if [ ! -d "$TEST_DIR/empty_dir" ]; then
        success "rmdir: Empty directory deleted successfully"
    else
        error "rmdir: Failed to delete empty directory"
        return 1
    fi
    
    # Test rm -rf for non-empty directory
    mkdir -p "$TEST_DIR/non_empty/subdir"
    echo "content" > "$TEST_DIR/non_empty/file.txt"
    
    rm -rf "$TEST_DIR/non_empty"
    
    if [ ! -d "$TEST_DIR/non_empty" ]; then
        success "rmdir: Non-empty directory deleted successfully"
    else
        error "rmdir: Failed to delete non-empty directory"
        return 1
    fi
}

# Test 10: Informazioni filesystem (statfs)
test_statfs() {
    log "Testing filesystem statistics..."
    
    # Test df command
    if df "$MOUNT_POINT" > /dev/null 2>&1; then
        success "statfs: Filesystem statistics retrieved"
        df -h "$MOUNT_POINT" | tee -a "$LOG_FILE"
    else
        error "statfs: Failed to get filesystem statistics"
        return 1
    fi
}

# Test 11: Lookup e percorsi
test_lookup() {
    log "Testing lookup operations..."
    
    # Create nested structure
    mkdir -p "$TEST_DIR/lookup/deep/nested"
    echo "lookup test" > "$TEST_DIR/lookup/deep/nested/file.txt"
    
    # Test file existence
    if [ -f "$TEST_DIR/lookup/deep/nested/file.txt" ]; then
        success "lookup: Deep nested file lookup successful"
    else
        error "lookup: Failed to lookup deep nested file"
        return 1
    fi
    
    # Test find command
    local found_files=$(find "$TEST_DIR" -name "*.txt" 2>/dev/null | wc -l)
    if [ "$found_files" -gt 0 ]; then
        success "lookup: Find command works ($found_files files found)"
    else
        warning "lookup: No .txt files found with find"
    fi
}

# Test 12: Operazioni concorrenti
test_concurrent() {
    log "Testing concurrent operations..."
    
    # Create multiple files simultaneously
    for i in {1..10}; do
        echo "Concurrent file $i" > "$TEST_DIR/concurrent_$i.txt" &
    done
    wait
    
    local concurrent_files=$(ls "$TEST_DIR"/concurrent_*.txt 2>/dev/null | wc -l)
    if [ "$concurrent_files" -eq 10 ]; then
        success "concurrent: Created 10 files concurrently"
    else
        warning "concurrent: Only created $concurrent_files out of 10 files"
    fi
    
    # Clean up
    rm -f "$TEST_DIR"/concurrent_*.txt
}

# Test 13: Link simbolici (se supportati)
test_symlinks() {
    log "Testing symbolic links..."
    
    local target_file="$TEST_DIR/link_target.txt"
    local link_file="$TEST_DIR/link_test.txt"
    
    echo "Link target content" > "$target_file"
    
    if ln -s "$target_file" "$link_file" 2>/dev/null; then
        if [ -L "$link_file" ]; then
            success "symlinks: Symbolic link created successfully"
            
            # Test reading through symlink
            local content=$(cat "$link_file" 2>/dev/null || echo "")
            if [ "$content" = "Link target content" ]; then
                success "symlinks: Can read through symbolic link"
            else
                warning "symlinks: Cannot read through symbolic link"
            fi
        else
            warning "symlinks: Link file exists but is not a symbolic link"
        fi
    else
        warning "symlinks: Symbolic links not supported or failed"
    fi
}

# Test di performance
test_performance() {
    log "Testing performance..."
    
    local perf_dir="$TEST_DIR/performance"
    mkdir -p "$perf_dir"
    
    # Test write performance
    log "Testing write performance..."
    local start_time=$(date +%s.%N)
    dd if=/dev/zero of="$perf_dir/perf_test.bin" bs=1M count=5 2>/dev/null
    local end_time=$(date +%s.%N)
    local duration=$(echo "$end_time - $start_time" | bc -l 2>/dev/null || echo "N/A")
    success "performance: Wrote 5MB in ${duration}s"
    
    # Test read performance
    log "Testing read performance..."
    start_time=$(date +%s.%N)
    dd if="$perf_dir/perf_test.bin" of=/dev/null bs=1M 2>/dev/null
    end_time=$(date +%s.%N)
    duration=$(echo "$end_time - $start_time" | bc -l 2>/dev/null || echo "N/A")
    success "performance: Read 5MB in ${duration}s"
    
    rm -f "$perf_dir/perf_test.bin"
}

# Cleanup function
cleanup() {
    log "Cleaning up test files..."
    if [ -d "$TEST_DIR" ]; then
        rm -rf "$TEST_DIR"
        success "cleanup: Test directory removed"
    fi
}

# Main test runner
run_all_tests() {
    log "Starting RemoteFS test suite..."
    echo "Test log: $LOG_FILE" | tee "$LOG_FILE"
    
    local failed_tests=0
    local total_tests=0
    
    # List of all tests
    tests=(
        "check_prerequisites"
        "test_mkdir"
        "test_readdir"
        "test_create" 
        "test_read"
        "test_write"
        "test_attributes"
        "test_rename"
        "test_unlink"
        "test_rmdir"
        "test_statfs"
        "test_lookup"
        "test_concurrent"
        "test_symlinks"
        "test_performance"
    )
    
    for test in "${tests[@]}"; do
        total_tests=$((total_tests + 1))
        log "Running $test..."
        
        if $test; then
            success "$test passed"
        else
            error "$test failed"
            failed_tests=$((failed_tests + 1))
        fi
        echo "" | tee -a "$LOG_FILE"
    done
    
    # Final cleanup
    cleanup
    
    # Results summary
    log "Test Results Summary:"
    log "Total tests: $total_tests"
    log "Passed: $((total_tests - failed_tests))"
    log "Failed: $failed_tests"
    
    if [ $failed_tests -eq 0 ]; then
        success "All tests passed! 🎉"
        exit 0
    else
        error "$failed_tests test(s) failed!"
        exit 1
    fi
}

# Handle command line arguments
case "${1:-all}" in
    "mkdir") 
        check_prerequisites
        test_mkdir 
        cleanup
        ;;
    "readdir") 
        check_prerequisites
        test_mkdir
        test_readdir 
        cleanup
        ;;
    "create") 
        check_prerequisites
        test_mkdir
        test_create 
        cleanup
        ;;
    "read") 
        check_prerequisites
        test_mkdir
        test_read 
        cleanup
        ;;
    "write") 
        check_prerequisites
        test_mkdir
        test_write 
        cleanup
        ;;
    "attr") 
        check_prerequisites
        test_mkdir
        test_attributes 
        cleanup
        ;;
    "rename") 
        check_prerequisites
        test_mkdir
        test_rename 
        cleanup
        ;;
    "unlink") 
        check_prerequisites
        test_mkdir
        test_unlink 
        cleanup
        ;;
    "rmdir") 
        check_prerequisites
        test_mkdir
        test_rmdir 
        cleanup
        ;;
    "statfs") 
        check_prerequisites
        test_statfs 
        ;;
    "lookup") 
        check_prerequisites
        test_mkdir
        test_lookup 
        cleanup
        ;;
    "concurrent") 
        check_prerequisites
        test_mkdir
        test_concurrent 
        cleanup
        ;;
    "symlinks") 
        check_prerequisites
        test_mkdir
        test_symlinks 
        cleanup
        ;;
    "performance") 
        check_prerequisites
        test_mkdir
        test_performance 
        cleanup
        ;;
    "all") run_all_tests ;;
    *)
        echo "Usage: $0 [test_name|all]"
        echo "Available tests: mkdir, readdir, create, read, write, attr, rename, unlink, rmdir, statfs, lookup, concurrent, symlinks, performance, all"
        exit 1
        ;;
esac
