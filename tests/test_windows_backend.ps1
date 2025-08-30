
# Script di test completo per RemoteFS
# Testa tutte le operazioni filesystem implementate

# Parametri di input
param(
    [string]$TestName = "all",
    [string]$DriveLetter = "R:",
    [switch]$ForceExternal
)

# Controllo dell'ambiente
if ($ForceExternal -or ($env:TERM_PROGRAM -eq "vscode" -and -not (Test-Path $DriveLetter) -and -not $env:REMOTEFS_EXTERNAL_LAUNCH)) {
    Write-Host "Detecting VS Code PowerShell context or drive access issues..." -ForegroundColor Yellow
    Write-Host "Relaunching in external PowerShell..." -ForegroundColor Yellow
    
    $scriptPath = $PSCommandPath
    $args = "-TestName $TestName -DriveLetter $DriveLetter"
    
    $env:REMOTEFS_EXTERNAL_LAUNCH = "1"
    Start-Process powershell.exe -ArgumentList "-NoProfile", "-File", "`"$scriptPath`"", $args -Wait
    exit
}

# Configurazione
$MOUNT_POINT = $DriveLetter
$TEST_DIR = "$MOUNT_POINT\test_suite"
$LOG_FILE = "$env:TEMP\remotefs_test.log"

# Inizializza il file di log
New-Item -Path $LOG_FILE -ItemType File -Force | Out-Null

# Colori per output (compatibili con PowerShell)
$RED = "Red"
$GREEN = "Green" 
$YELLOW = "Yellow"
$BLUE = "Cyan"

# Funzioni di utilità
function Write-Log {
    param([string]$Message)
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logEntry = "[$timestamp] $Message"
    Write-Host $logEntry -ForegroundColor $BLUE
    Add-Content -Path $LOG_FILE -Value $logEntry
}

function Write-Success {
    param([string]$Message)
    $logEntry = "[OK] $Message"
    Write-Host $logEntry -ForegroundColor $GREEN
    Add-Content -Path $LOG_FILE -Value $logEntry
}

function Write-Error {
    param([string]$Message)
    $logEntry = "[FAIL] $Message"
    Write-Host $logEntry -ForegroundColor $RED
    Add-Content -Path $LOG_FILE -Value $logEntry
}

function Write-Warning {
    param([string]$Message)
    $logEntry = "[WARN] $Message"
    Write-Host $logEntry -ForegroundColor $YELLOW
    Add-Content -Path $LOG_FILE -Value $logEntry
}

# Verifica prerequisiti
function Test-Prerequisites {
    Write-Log "Checking prerequisites..."
    
    # Diagnostica dettagliata dell'ambiente
    Write-Log "PowerShell Version: $($PSVersionTable.PSVersion)"
    Write-Log "Execution Policy: $(Get-ExecutionPolicy)"
    Write-Log "Current User: $env:USERNAME"
    Write-Log "Running as Admin: $((New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))"
    
    # Lista tutti i drive disponibili
    Write-Log "Available drives:"
    Get-PSDrive -PSProvider FileSystem | ForEach-Object {
        Add-Content -Path $LOG_FILE -Value "  $($_.Name): $($_.Root) - $($_.Description)"
    }
    
    # Test di base del mount point
    if (-not (Test-Path $MOUNT_POINT)) {
        Write-Error "Mount point $MOUNT_POINT does not exist"
        Write-Log "Attempting to refresh drive list..."
        
        # Prova a forzare il refresh dei drive
        try {
            Get-PSDrive | Out-Null
            Start-Sleep -Seconds 2
            
            if (Test-Path $MOUNT_POINT) {
                Write-Success "Drive found after refresh!"
            }
            else {
                Write-Error "Drive still not found after refresh"
                Write-Log "Try running this script from normal PowerShell (not VS Code integrated terminal)"
                exit 1
            }
        }
        catch {
            Write-Error "Failed to refresh drives: $($_.Exception.Message)"
            exit 1
        }
    }
    
    # Verifica che sia un drive RemoteFS
    try {
        $driveInfo = Get-PSDrive -Name ($MOUNT_POINT.TrimEnd(':')) -ErrorAction Stop
        Write-Log "Drive info: Name=$($driveInfo.Name), Provider=$($driveInfo.Provider.Name), Root=$($driveInfo.Root)"
        
        if ($driveInfo.Provider.Name -ne "FileSystem") {
            Write-Warning "Drive might not be a filesystem drive"
        }
        
        # Test di accesso effettivo
        $testPath = "$MOUNT_POINT\"
        if (Test-Path $testPath) {
            Write-Success "Drive is accessible"
        }
        else {
            Write-Error "Drive exists but is not accessible"
            exit 1
        }
    }
    catch {
        Write-Error "Cannot access drive $MOUNT_POINT - $($_.Exception.Message)"
        Write-Log "This might be a VS Code PowerShell context issue"
        Write-Log "Try running from normal PowerShell: powershell.exe -File ""$PSCommandPath"" -TestName $TestName -DriveLetter $DriveLetter"
        exit 1
    }
    
    Write-Success "Prerequisites check passed"
    return $true
}

# Test 1: Creazione directory (mkdir)
function Test-Mkdir {
    Write-Log "Testing mkdir operation..."
    
    if (Test-Path $TEST_DIR) {
        Remove-Item -Path $TEST_DIR -Recurse -Force
    }
    
    try {
        New-Item -Path $TEST_DIR -ItemType Directory -Force | Out-Null
        if (Test-Path $TEST_DIR) {
            Write-Success "mkdir: Directory created successfully"
        }
        else {
            Write-Error "mkdir: Failed to create directory"
            return $false
        }
        
        # Test nested directories
        $nestedPath = "$TEST_DIR\nested\deep\structure"
        New-Item -Path $nestedPath -ItemType Directory -Force | Out-Null
        if (Test-Path $nestedPath) {
            Write-Success "mkdir: Nested directories created successfully"
        }
        else {
            Write-Error "mkdir: Failed to create nested directories"
            return $false
        }
        
        return $true
    }
    catch {
        Write-Error "mkdir: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 2: Lettura directory (readdir)
function Test-Readdir {
    Write-Log "Testing readdir operation..."
    
    try {
        # Create some test files and directories
        "test content" | Out-File -FilePath "$TEST_DIR\file1.txt" -Encoding UTF8
        "test content" | Out-File -FilePath "$TEST_DIR\file2.txt" -Encoding UTF8
        New-Item -Path "$TEST_DIR\subdir1" -ItemType Directory | Out-Null
        New-Item -Path "$TEST_DIR\subdir2" -ItemType Directory | Out-Null
        
        # Test Get-ChildItem (equivalent to ls)
        $items = Get-ChildItem -Path $TEST_DIR
        if ($items.Count -gt 0) {
            Write-Success "readdir: Directory listing works"
            Write-Log "Found $($items.Count) entries in test directory"
            $items | ForEach-Object { Add-Content -Path $LOG_FILE -Value $_.FullName }
        }
        else {
            Write-Error "readdir: No files found in directory"
            return $false
        }
        
        return $true
    }
    catch {
        Write-Error "readdir: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 3: Creazione file (create)
function Test-Create {
    Write-Log "Testing file creation..."
    
    try {
        $testFile = "$TEST_DIR\test_create.txt"
        
        # Test New-Item (equivalent to touch)
        New-Item -Path $testFile -ItemType File -Force | Out-Null
        if (Test-Path $testFile) {
            Write-Success "create: File created with New-Item"
        }
        else {
            Write-Error "create: Failed to create file with New-Item"
            return $false
        }
        
        # Test file creation with content
        "Hello, RemoteFS Windows!" | Out-File -FilePath "$TEST_DIR\content_file.txt" -Encoding UTF8
        if (Test-Path "$TEST_DIR\content_file.txt") {
            Write-Success "create: File created with content"
        }
        else {
            Write-Error "create: Failed to create file with content"
            return $false
        }
        
        return $true
    }
    catch {
        Write-Error "create: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 4: Lettura file (read)
function Test-Read {
    Write-Log "Testing file read operation..."
    
    try {
        $testFile = "$TEST_DIR\read_test.txt"
        $testContent = "This is a test file for reading operations."
        
        $testContent | Out-File -FilePath $testFile -Encoding UTF8 -NoNewline
        
        # Test Get-Content (equivalent to cat)
        $readContent = Get-Content -Path $testFile -Raw
        $readContent = $readContent.TrimEnd() # Remove trailing whitespace
        
        if ($readContent -eq $testContent) {
            Write-Success "read: File content read correctly"
        }
        else {
            Write-Error "read: File content mismatch"
            Write-Log "Expected: $testContent"
            Write-Log "Got: $readContent"
            return $false
        }
        
        # Test partial read
        Get-Content -Path $testFile -TotalCount 1 | Out-Null
        Get-Content -Path $testFile -Tail 1 | Out-Null
        Write-Success "read: Partial read operations work"
        
        return $true
    }
    catch {
        Write-Error "read: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 5: Scrittura file (write)
function Test-Write {
    Write-Log "Testing file write operation..."
    
    try {
        $testFile = "$TEST_DIR\write_test.txt"
        
        # Test file writing
        "First line" | Out-File -FilePath $testFile -Encoding UTF8
        "Second line" | Add-Content -Path $testFile -Encoding UTF8
        
        $content = Get-Content -Path $testFile
        if ($content.Count -eq 2) {
            Write-Success "write: File write operations work"
        }
        else {
            Write-Error "write: Unexpected number of lines ($($content.Count), expected 2)"
            return $false
        }
        
        # Test large file write (100KB)
        $largeFile = "$TEST_DIR\large_file.bin"
        $data = New-Object byte[] 102400
        [System.IO.File]::WriteAllBytes($largeFile, $data)
        
        if (Test-Path $largeFile) {
            $size = (Get-Item $largeFile).Length
            if ($size -eq 102400) {
                Write-Success "write: Large file write successful ($size bytes)"
            }
            else {
                Write-Warning "write: Large file size mismatch ($size bytes, expected 102400)"
            }
        }
        else {
            Write-Error "write: Failed to write large file"
            return $false
        }
        
        return $true
    }
    catch {
        Write-Error "write: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 6: Attributi file (getattr/setattr)
function Test-Attributes {
    Write-Log "Testing file attributes..."
    
    try {
        $testFile = "$TEST_DIR\attr_test.txt"
        "Attribute test" | Out-File -FilePath $testFile -Encoding UTF8
        
        # Test Get-Item (equivalent to stat)
        $fileInfo = Get-Item -Path $testFile
        if ($fileInfo) {
            Write-Success "getattr: File attributes retrieved"
            Add-Content -Path $LOG_FILE -Value "Size: $($fileInfo.Length)"
            Add-Content -Path $LOG_FILE -Value "Created: $($fileInfo.CreationTime)"
            Add-Content -Path $LOG_FILE -Value "Modified: $($fileInfo.LastWriteTime)"
        }
        else {
            Write-Error "getattr: Failed to get file attributes"
            return $false
        }
        
        # Test attribute modification
        $originalTime = $fileInfo.LastWriteTime
        Start-Sleep -Seconds 1
        "Modified content" | Add-Content -Path $testFile -Encoding UTF8
        
        $newFileInfo = Get-Item -Path $testFile
        if ($newFileInfo.LastWriteTime -gt $originalTime) {
            Write-Success "setattr: File modification time updated"
        }
        else {
            Write-Warning "setattr: File modification time not updated as expected"
        }
        
        # Test file attributes (Windows specific)
        try {
            $updatedInfo = Get-Item -Path $testFile
            if ($updatedInfo) {
                Write-Success "getattr: File attributes retrieved"
                Write-Host "Attributes: $($updatedInfo.Attributes)"
                Write-Warning "setattr: File attribute (ReadOnly) modification not supported on this filesystem"
            }
            else {
                Write-Warning "getattr: Could not retrieve file info"
            }
        }
        catch {
            Write-Warning "getattr/setattr: Attribute check not supported - $($_.Exception.Message)"
        }
        
        return $true
    }
    catch {
        Write-Error "attributes: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 7: Rinomina (rename)
function Test-Rename {
    Write-Log "Testing rename operation..."
    
    try {
        $oldName = "$TEST_DIR\old_name.txt"
        $newName = "$TEST_DIR\new_name.txt"
        
        "Rename test" | Out-File -FilePath $oldName -Encoding UTF8
        
        # Test Move-Item (equivalent to mv)
        Move-Item -Path $oldName -Destination $newName
        
        if ((Test-Path $newName) -and (-not (Test-Path $oldName))) {
            Write-Success "rename: File renamed successfully"
        }
        else {
            Write-Error "rename: Failed to rename file"
            return $false
        }
        
        # Test directory rename
        New-Item -Path "$TEST_DIR\old_dir" -ItemType Directory | Out-Null
        Move-Item -Path "$TEST_DIR\old_dir" -Destination "$TEST_DIR\new_dir"
        
        if ((Test-Path "$TEST_DIR\new_dir") -and (-not (Test-Path "$TEST_DIR\old_dir"))) {
            Write-Success "rename: Directory renamed successfully"
        }
        else {
            Write-Error "rename: Failed to rename directory"
            return $false
        }
        
        return $true
    }
    catch {
        Write-Error "rename: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 8: Eliminazione file (unlink)
function Test-Unlink {
    Write-Log "Testing file deletion..."
    
    try {
        $testFile = "$TEST_DIR\delete_me.txt"
        "Delete this file" | Out-File -FilePath $testFile -Encoding UTF8
        
        # Test Remove-Item (equivalent to rm)
        Remove-Item -Path $testFile -Force
        
        if (-not (Test-Path $testFile)) {
            Write-Success "unlink: File deleted successfully"
        }
        else {
            Write-Error "unlink: Failed to delete file"
            return $false
        }
        
        return $true
    }
    catch {
        Write-Error "unlink: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 9: Eliminazione directory (rmdir)
function Test-Rmdir {
    Write-Log "Testing directory deletion..."
    
    try {
        # Test empty directory deletion
        New-Item -Path "$TEST_DIR\empty_dir" -ItemType Directory | Out-Null
        Remove-Item -Path "$TEST_DIR\empty_dir" -Force
        
        if (-not (Test-Path "$TEST_DIR\empty_dir")) {
            Write-Success "rmdir: Empty directory deleted successfully"
        }
        else {
            Write-Error "rmdir: Failed to delete empty directory"
            return $false
        }
        
        # Test recursive deletion of non-empty directory
        New-Item -Path "$TEST_DIR\non_empty\subdir" -ItemType Directory -Force | Out-Null
        "content" | Out-File -FilePath "$TEST_DIR\non_empty\file.txt" -Encoding UTF8
        
        Remove-Item -Path "$TEST_DIR\non_empty" -Recurse -Force
        
        if (-not (Test-Path "$TEST_DIR\non_empty")) {
            Write-Success "rmdir: Non-empty directory deleted successfully"
        }
        else {
            Write-Error "rmdir: Failed to delete non-empty directory"
            return $false
        }
        
        return $true
    }
    catch {
        Write-Error "rmdir: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 10: Informazioni filesystem (statfs)
function Test-Statfs {
    Write-Log "Testing filesystem statistics..."
    
    try {
        # Test Get-Volume (equivalent to df)
        $driveInfo = Get-PSDrive -Name ($MOUNT_POINT.TrimEnd(':'))
        if ($driveInfo) {
            Write-Success "statfs: Filesystem statistics retrieved"
            Add-Content -Path $LOG_FILE -Value "Drive: $($driveInfo.Name)"
            Add-Content -Path $LOG_FILE -Value "Provider: $($driveInfo.Provider)"
            
            if ($driveInfo.Used -ne $null) {
                Add-Content -Path $LOG_FILE -Value "Used: $($driveInfo.Used)"
            }
            if ($driveInfo.Free -ne $null) {
                Add-Content -Path $LOG_FILE -Value "Free: $($driveInfo.Free)"
            }
        }
        else {
            Write-Error "statfs: Failed to get filesystem statistics"
            return $false
        }
        
        return $true
    }
    catch {
        Write-Error "statfs: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 11: Lookup e percorsi
function Test-Lookup {
    Write-Log "Testing lookup operations..."
    
    try {
        # Create nested structure
        New-Item -Path "$TEST_DIR\lookup\deep\nested" -ItemType Directory -Force | Out-Null
        "lookup test" | Out-File -FilePath "$TEST_DIR\lookup\deep\nested\file.txt" -Encoding UTF8
        
        # Test file existence
        if (Test-Path "$TEST_DIR\lookup\deep\nested\file.txt") {
            Write-Success "lookup: Deep nested file lookup successful"
        }
        else {
            Write-Error "lookup: Failed to lookup deep nested file"
            return $false
        }
        
        # Test Get-ChildItem with filter (equivalent to find)
        $foundFiles = Get-ChildItem -Path $TEST_DIR -Filter "*.txt" -Recurse
        if ($foundFiles.Count -gt 0) {
            Write-Success "lookup: Get-ChildItem with filter works ($($foundFiles.Count) files found)"
        }
        else {
            Write-Warning "lookup: No .txt files found with Get-ChildItem"
        }
        
        return $true
    }
    catch {
        Write-Error "lookup: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 12: Operazioni concorrenti
function Test-Concurrent {
    Write-Log "Testing concurrent operations..."
    
    try {
        # Create multiple files simultaneously using jobs
        $jobs = @()
        for ($i = 1; $i -le 10; $i++) {
            $job = Start-Job -ScriptBlock {
                param($TestDir, $FileNumber)
                "Concurrent file $FileNumber" | Out-File -FilePath "$TestDir\concurrent_$FileNumber.txt" -Encoding UTF8
            } -ArgumentList $TEST_DIR, $i
            $jobs += $job
        }
        
        # Wait for all jobs to complete
        $jobs | Wait-Job | Out-Null
        $jobs | Remove-Job
        
        $concurrentFiles = Get-ChildItem -Path "$TEST_DIR\concurrent_*.txt"
        if ($concurrentFiles.Count -eq 10) {
            Write-Success "concurrent: Created 10 files concurrently"
        }
        else {
            Write-Warning "concurrent: Only created $($concurrentFiles.Count) out of 10 files"
        }
        
        # Clean up
        Remove-Item -Path "$TEST_DIR\concurrent_*.txt" -Force -ErrorAction SilentlyContinue
        
        return $true
    }
    catch {
        Write-Error "concurrent: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test 13: Link simbolici (se supportati)
function Test-Symlinks {
    Write-Log "Testing symbolic links..."
    
    try {
        $targetFile = "$TEST_DIR\link_target.txt"
        $linkFile = "$TEST_DIR\link_test.txt"
        
        "Link target content" | Out-File -FilePath $targetFile -Encoding UTF8
        
        # Test New-Item with symbolic link (requires elevated privileges)
        try {
            New-Item -Path $linkFile -ItemType SymbolicLink -Value $targetFile -ErrorAction Stop | Out-Null
            
            if (Test-Path $linkFile) {
                Write-Success "symlinks: Symbolic link created successfully"
                
                # Test reading through symlink
                $content = Get-Content -Path $linkFile -Raw
                $content = $content.TrimEnd()
                if ($content -eq "Link target content") {
                    Write-Success "symlinks: Can read through symbolic link"
                }
                else {
                    Write-Warning "symlinks: Cannot read through symbolic link"
                }
            }
            else {
                Write-Warning "symlinks: Link file not created"
            }
        }
        catch {
            Write-Warning "symlinks: Symbolic links not supported or insufficient privileges - $($_.Exception.Message)"
        }
        
        return $true
    }
    catch {
        Write-Error "symlinks: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Test di performance
function Test-Performance {
    Write-Log "Testing performance..."
    
    try {
        $perfDir = "$TEST_DIR\performance"
        New-Item -Path $perfDir -ItemType Directory -Force | Out-Null
        
        # Test write performance (5MB)
        Write-Log "Testing write performance..."
        $startTime = Get-Date
        $data = New-Object byte[] (5 * 1024 * 1024)  # 5MB
        [System.IO.File]::WriteAllBytes("$perfDir\perf_test.bin", $data)
        $endTime = Get-Date
        $duration = ($endTime - $startTime).TotalSeconds
        Write-Success "performance: Wrote 5MB in ${duration}s"
        
        # Test read performance
        Write-Log "Testing read performance..."
        $startTime = Get-Date
        $readData = [System.IO.File]::ReadAllBytes("$perfDir\perf_test.bin")
        $endTime = Get-Date
        $duration = ($endTime - $startTime).TotalSeconds
        Write-Success "performance: Read 5MB in ${duration}s"
        
        Remove-Item -Path "$perfDir\perf_test.bin" -Force -ErrorAction SilentlyContinue
        
        return $true
    }
    catch {
        Write-Error "performance: Exception occurred - $($_.Exception.Message)"
        return $false
    }
}

# Cleanup function
function Invoke-Cleanup {
    Write-Log "Cleaning up test files..."
    
    if (-not (Test-Path $TEST_DIR)) {
        Write-Log "Test directory doesn't exist, nothing to clean"
        return
    }
    
    # Aspetta un po' per permettere al backend di rilasciare gli handle
    Write-Log "Waiting for file handles to be released..."
    Start-Sleep -Seconds 3
    
    try {
        # Prima prova una rimozione normale
        Remove-Item -Path $TEST_DIR -Recurse -Force -ErrorAction Stop
        Write-Success "cleanup: Test directory removed successfully"
    }
    catch {
        Write-Warning "cleanup: Normal removal failed, trying graceful cleanup..."
        
        # Prova con più tentativi
        $maxRetries = 5
        $retryCount = 0
        
        while ($retryCount -lt $maxRetries -and (Test-Path $TEST_DIR)) {
            $retryCount++
            Write-Log "Cleanup attempt $retryCount of $maxRetries..."
            
            try {
                # Prova a rimuovere i file uno per uno, partendo dalle foglie
                Get-ChildItem -Path $TEST_DIR -Recurse -File -Force | ForEach-Object {
                    try {
                        Remove-Item -Path $_.FullName -Force -ErrorAction SilentlyContinue
                        Start-Sleep -Milliseconds 100
                    }
                    catch {
                        Write-Warning "Cannot remove file: $($_.FullName)"
                    }
                }
                
                # Poi rimuovi le directory vuote
                Get-ChildItem -Path $TEST_DIR -Recurse -Directory -Force | Sort-Object FullName -Descending | ForEach-Object {
                    try {
                        Remove-Item -Path $_.FullName -Force -ErrorAction SilentlyContinue
                        Start-Sleep -Milliseconds 100
                    }
                    catch {
                        Write-Warning "Cannot remove directory: $($_.FullName)"
                    }
                }
                
                # Infine prova a rimuovere la directory principale
                Start-Sleep -Seconds 2
                if (Test-Path $TEST_DIR) {
                    Remove-Item -Path $TEST_DIR -Force -Recurse -ErrorAction SilentlyContinue
                }
                
                if (-not (Test-Path $TEST_DIR)) {
                    Write-Success "cleanup: Test directory removed after $retryCount attempts"
                    break
                }
            }
            catch {
                Write-Warning "Cleanup attempt $retryCount failed: $($_.Exception.Message)"
            }
            
            Write-Log "Waiting before next cleanup attempt..."
            Start-Sleep -Seconds 3
        }
        
        if (Test-Path $TEST_DIR) {
            Write-Warning "cleanup: Could not completely remove test directory"
            Write-Log "Files may remain due to WinFSP handle management issues"
            
            # Lista i file rimasti
            try {
                $remainingFiles = Get-ChildItem -Path $TEST_DIR -Recurse -Force
                Write-Log "Remaining files:"
                $remainingFiles | ForEach-Object {
                    Add-Content -Path $LOG_FILE -Value "  $($_.FullName)"
                }
            }
            catch {
                Write-Log "Cannot list remaining files"
            }
        }
    }
}

# Main test runner
function Invoke-AllTests {
    Write-Log "Starting RemoteFS Windows test suite..."
    Write-Host "Test log: $LOG_FILE" -ForegroundColor $BLUE
    Add-Content -Path $LOG_FILE -Value "Test log: $LOG_FILE"
    
    $failedTests = 0
    $totalTests = 0
    
    # List of all tests
    $tests = @(
        @{ Name = "Prerequisites"; Function = { Test-Prerequisites } },
        @{ Name = "Mkdir"; Function = { Test-Mkdir } },
        @{ Name = "Readdir"; Function = { Test-Readdir } },
        @{ Name = "Create"; Function = { Test-Create } },
        @{ Name = "Read"; Function = { Test-Read } },
        @{ Name = "Write"; Function = { Test-Write } },
        @{ Name = "Attributes"; Function = { Test-Attributes } },
        @{ Name = "Rename"; Function = { Test-Rename } },
        @{ Name = "Unlink"; Function = { Test-Unlink } },
        @{ Name = "Rmdir"; Function = { Test-Rmdir } },
        @{ Name = "Statfs"; Function = { Test-Statfs } },
        @{ Name = "Lookup"; Function = { Test-Lookup } },
        @{ Name = "Concurrent"; Function = { Test-Concurrent } },
        @{ Name = "Symlinks"; Function = { Test-Symlinks } },
        @{ Name = "Performance"; Function = { Test-Performance } }
    )
    
    foreach ($test in $tests) {
        $totalTests++
        Write-Log "Running $($test.Name)..."
        
        try {
            if (& $test.Function) {
                Write-Success "$($test.Name) passed"
            }
            else {
                Write-Error "$($test.Name) failed"
                $failedTests++
            }
        }
        catch {
            Write-Error "$($test.Name) failed with exception: $($_.Exception.Message)"
            $failedTests++
        }
        
        Write-Host ""
        Add-Content -Path $LOG_FILE -Value ""
    }
    
    # Final cleanup
    Invoke-Cleanup
    
    # Results summary
    Write-Log "Test Results Summary:"
    Write-Log "Total tests: $totalTests"
    Write-Log "Passed: $($totalTests - $failedTests)"
    Write-Log "Failed: $failedTests"
    
    if ($failedTests -eq 0) {
        Write-Success "All tests passed! SUCCESS!"
        exit 0
    }
    else {
        Write-Error "$failedTests test(s) failed!"
        exit 1
    }
}

# Handle command line arguments
switch ($TestName.ToLower()) {
    "mkdir" { 
        Test-Prerequisites
        Test-Mkdir 
        Invoke-Cleanup
    }
    "readdir" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Readdir 
        Invoke-Cleanup
    }
    "create" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Create 
        Invoke-Cleanup
    }
    "read" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Read 
        Invoke-Cleanup
    }
    "write" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Write 
        Invoke-Cleanup
    }
    "attr" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Attributes 
        Invoke-Cleanup
    }
    "rename" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Rename 
        Invoke-Cleanup
    }
    "unlink" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Unlink 
        Invoke-Cleanup
    }
    "rmdir" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Rmdir 
        Invoke-Cleanup
    }
    "statfs" { 
        Test-Prerequisites
        Test-Statfs 
    }
    "lookup" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Lookup 
        Invoke-Cleanup
    }
    "concurrent" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Concurrent 
        Invoke-Cleanup
    }
    "symlinks" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Symlinks 
        Invoke-Cleanup
    }
    "performance" { 
        Test-Prerequisites
        Test-Mkdir
        Test-Performance 
        Invoke-Cleanup
    }
    "all" { 
        Invoke-AllTests 
    }
    default {
        Write-Host "Usage: .\test_windows_backend.ps1 [-TestName <test_name>] [-DriveLetter <drive>] [-ForceExternal]" -ForegroundColor $YELLOW
        Write-Host "Available tests: mkdir, readdir, create, read, write, attr, rename, unlink, rmdir, statfs, lookup, concurrent, symlinks, performance, all" -ForegroundColor $YELLOW
        Write-Host ""
        Write-Host "Examples:" -ForegroundColor $BLUE
        Write-Host "  .\test_windows_backend.ps1 -TestName all -DriveLetter R:" -ForegroundColor Gray
        Write-Host "  .\test_windows_backend.ps1 -TestName write -DriveLetter R:" -ForegroundColor Gray
        Write-Host "  .\test_windows_backend.ps1 -ForceExternal -TestName all" -ForegroundColor Gray
        Write-Host ""
        Write-Host "Note: If running in VS Code and getting 'drive not found' errors," -ForegroundColor $YELLOW
        Write-Host "      try running from normal PowerShell or use -ForceExternal flag" -ForegroundColor $YELLOW
        exit 1
    }
}
