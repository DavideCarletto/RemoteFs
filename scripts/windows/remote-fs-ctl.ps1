#!/usr/bin/env pwsh
# Script per controllare il client RemoteFS

param(
    [string]$Action = "status",
    [string]$DriveFilter = "R:"
)

# Configurazione percorsi
$scriptRoot = $PSScriptRoot
$projectRoot = Split-Path (Split-Path $scriptRoot -Parent) -Parent
$clientDir = Join-Path $projectRoot "client"

$PID_FILE = Join-Path $env:TEMP "remote-fs-client.pid"
$LOG_FILE = Join-Path $env:TEMP "remote-fs-client.log"

function Get-RemoteFsProcess {
    $processes = Get-Process -Name "remote_fs" -ErrorAction SilentlyContinue
    return $processes
}

function Get-PidFromFile {
    if (Test-Path $PID_FILE) {
        try {
            return [int](Get-Content $PID_FILE -Raw).Trim()
        } catch {
            return $null
        }
    }
    return $null
}

function Open-NewShellForDrive {
    Write-Host "🚀 Apertura nuova finestra PowerShell..." -ForegroundColor Green
    Start-Process powershell -ArgumentList '-NoExit', '-Command', 'Write-Host "Drive R: disponibile!"; cd R:; dir'
}

function Copy-WinFspDll {
    # Percorsi WinFSP
    $winFspPath = "C:\Program Files (x86)\WinFsp"
    $winFspDll = Join-Path $winFspPath "bin\winfsp-x64.dll"
    $targetDir = Join-Path $clientDir "target\release"
    $targetDll = Join-Path $targetDir "winfsp-x64.dll"
    
    # Controlla se WinFSP è installato
    if (-not (Test-Path $winFspPath)) {
        Write-Host "⚠️ ERRORE: WinFSP non trovato in $winFspPath" -ForegroundColor Red
        Write-Host "Installa WinFSP da: https://winfsp.dev/rel/" -ForegroundColor Yellow
        return $false
    }
    
    # Controlla se la DLL di WinFSP esiste
    if (-not (Test-Path $winFspDll)) {
        Write-Host "⚠️ ERRORE: DLL WinFSP non trovata: $winFspDll" -ForegroundColor Red
        return $false
    }
    
    # Crea la directory target/release se non esiste
    if (-not (Test-Path $targetDir)) {
        Write-Host "📁 Creazione directory: $targetDir" -ForegroundColor Yellow
        New-Item -Path $targetDir -ItemType Directory -Force | Out-Null
    }
    
    # Copia la DLL se non esiste o se è diversa
    if (-not (Test-Path $targetDll) -or 
        (Get-FileHash $winFspDll -Algorithm MD5).Hash -ne (Get-FileHash $targetDll -Algorithm MD5).Hash) {
        
        Write-Host "📋 Copia DLL WinFSP: $winFspDll -> $targetDll" -ForegroundColor Yellow
        try {
            Copy-Item $winFspDll $targetDll -Force
            Write-Host "✅ DLL WinFSP copiata con successo" -ForegroundColor Green
        } catch {
            Write-Host "❌ Errore durante la copia della DLL: $($_.Exception.Message)" -ForegroundColor Red
            return $false
        }
    } else {
        Write-Host "✅ DLL WinFSP già presente e aggiornata" -ForegroundColor Green
    }
    
    return $true
}

function Start-RemoteFs {
    $existing = Get-RemoteFsProcess
    if ($existing) {
        Write-Host "❌ RemoteFS già in esecuzione (PID: $($existing.Id -join ', '))" -ForegroundColor Red
        return
    }

    Write-Host "🚀 Avvio RemoteFS daemon..." -ForegroundColor Green
    
    if (-not (Test-Path $clientDir)) {
        Write-Host "❌ Directory client non trovata: $clientDir" -ForegroundColor Red
        Write-Host "ℹ️  Assicurati di essere nella directory root del progetto RemoteFS" -ForegroundColor Yellow
        return
    }

    # Copia la DLL WinFSP prima di avviare il daemon
    Write-Host "🔧 Verifica dipendenze WinFSP..." -ForegroundColor Yellow
    if (-not (Copy-WinFspDll)) {
        Write-Host "❌ Impossibile copiare la DLL WinFSP. Avvio annullato." -ForegroundColor Red
        return
    }

    Write-Host "🚀 Avvio daemon in background..." -ForegroundColor Yellow
    
    try {
        # Usa Start-Process per avviare completamente staccato
        $processInfo = Start-Process -FilePath "cargo" -ArgumentList "run", "--release" -WorkingDirectory $clientDir -WindowStyle Hidden -PassThru
        
        Write-Host "⏳ Attendendo avvio del daemon..." -ForegroundColor Yellow
        
        # Aspetta più tempo e controlla periodicamente
        $maxWait = 30  # secondi
        $checkInterval = 2  # secondi
        $attempts = 0
        $process = $null
        
        while ($attempts -lt ($maxWait / $checkInterval)) {
            Start-Sleep -Seconds $checkInterval
            $attempts++
            
            # Cerca il processo remote_fs
            $process = Get-RemoteFsProcess
            
            if ($process) {
                Write-Host "✅ Processo daemon trovato dopo $($attempts * $checkInterval)s (PID: $($process.Id))" -ForegroundColor Green
                break
            }
            
            # Mostra progresso ogni 10 secondi
            if (($attempts * $checkInterval) % 10 -eq 0) {
                Write-Host "⏳ Ancora in attesa... ($($attempts * $checkInterval)s/$maxWait s)" -ForegroundColor Yellow
            }
        }
        
        if ($process) {
            # Salva il PID
            $process.Id | Out-File -FilePath $PID_FILE -Encoding utf8
            
            # Aspetta un po' per dare tempo al mount
            Write-Host "📁 Filesystem in fase di montaggio su R:..." -ForegroundColor Yellow
            Start-Sleep -Seconds 3
            
            # Verifica che il processo sia ancora attivo
            $currentProcess = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
            if ($currentProcess) {
                Write-Host "✅ Daemon attivo e funzionante!" -ForegroundColor Green
                
                # Controlla se il drive è accessibile
                if (Test-Path "R:\") {
                    Write-Host "✅ Drive R: montato e accessibile" -ForegroundColor Green
                } else {
                    Write-Host "⚠️  Drive R: non ancora accessibile (normale durante l'avvio)" -ForegroundColor Yellow
                }
                
                Write-Host "📂 Usa: Get-ChildItem R: per accedere ai file" -ForegroundColor Cyan
                Write-Host "📂 Oppure: cd R: && dir" -ForegroundColor Cyan
                Write-Host "📋 Log: `"$LOG_FILE`"" -ForegroundColor Cyan
                Write-Host "🛑 Per fermare: .\remote-fs-ctl.ps1 stop" -ForegroundColor Yellow
                
                # Suggerisci di aprire una nuova finestra per vedere il drive
                Write-Host "" 
                Write-Host "💡 Suggerimento: Usa .\remote-fs-ctl.ps1 shell per aprire nuova finestra" -ForegroundColor Magenta
                
                return
            } else {
                Write-Host "❌ Il processo è terminato inaspettatamente" -ForegroundColor Red
                if (Test-Path $LOG_FILE) {
                    Write-Host "📋 Controlla il log: $LOG_FILE" -ForegroundColor Yellow
                }
                return
            }
        } else {
            Write-Host "❌ Timeout: processo daemon non trovato dopo ${maxWait}s" -ForegroundColor Red
            Write-Host "📋 Controlla manualmente: cargo run --release nella directory client/" -ForegroundColor Yellow
            return
        }
        
        # Rimuovi il job completato
        Remove-Job -Id $job.Id -Force -ErrorAction SilentlyContinue
        
    } catch {
        Write-Host "❌ Errore nell'avvio: $($_.Exception.Message)" -ForegroundColor Red
    }
}

function Stop-RemoteFs {
    $processes = Get-RemoteFsProcess
    $pidFromFile = Get-PidFromFile
    
    if (-not $processes -and -not $pidFromFile) {
        Write-Host "ℹ️  Nessun processo RemoteFS trovato" -ForegroundColor Yellow
        return
    }

    # Prova prima con il PID dal file
    if ($pidFromFile) {
        try {
            $proc = Get-Process -Id $pidFromFile -ErrorAction SilentlyContinue
            if ($proc -and $proc.ProcessName -eq "remote_fs") {
                Write-Host "🛑 Fermando RemoteFS (PID: $pidFromFile)..." -ForegroundColor Yellow
                $proc.Kill()
                $proc.WaitForExit(5000)
                Write-Host "✅ Processo fermato" -ForegroundColor Green
            }
        } catch {
            Write-Host "⚠️  Errore nel fermare il processo: $($_.Exception.Message)" -ForegroundColor Red
        }
    }

    # Forza la terminazione di tutti i processi remote_fs rimanenti
    $remaining = Get-RemoteFsProcess
    if ($remaining) {
        Write-Host "🔨 Terminazione forzata dei processi rimanenti..." -ForegroundColor Red
        $remaining | ForEach-Object { 
            try {
                $_.Kill()
                Write-Host "✅ Processo $($_.Id) terminato" -ForegroundColor Green
            } catch {
                Write-Host "❌ Errore nel terminare processo $($_.Id)" -ForegroundColor Red
            }
        }
    }

    # Pulisci il PID file
    if (Test-Path $PID_FILE) {
        Remove-Item $PID_FILE -Force
    }

    Write-Host "🧹 Cleanup completato" -ForegroundColor Green
}

function Show-Status {
    $processes = Get-RemoteFsProcess
    $pidFromFile = Get-PidFromFile
    
    Write-Host "📊 Stato RemoteFS:" -ForegroundColor Cyan
    Write-Host "─────────────────" -ForegroundColor Cyan
    
    if ($processes) {
        Write-Host "✅ Processi attivi:" -ForegroundColor Green
        $processes | ForEach-Object {
            Write-Host "   PID: $($_.Id) | CPU: $([math]::Round($_.CPU,2))s | Memory: $([math]::Round($_.WorkingSet64/1MB,1))MB" -ForegroundColor White
        }
    } else {
        Write-Host "❌ Nessun processo RemoteFS trovato" -ForegroundColor Red
    }
    
    if ($pidFromFile) {
        Write-Host "📄 PID file: $pidFromFile" -ForegroundColor Cyan
    } else {
        Write-Host "📄 PID file: Non trovato" -ForegroundColor Yellow
    }
    
    # Controlla i drive montati
    $drives = Get-WmiObject -Class Win32_LogicalDisk | Where-Object { $_.DeviceID -like "*:" -and $_.FileSystem -eq $null }
    if ($drives) {
        Write-Host "💾 Drive potenzialmente montati da RemoteFS:" -ForegroundColor Cyan
        $drives | ForEach-Object {
            try {
                $content = Get-ChildItem "$($_.DeviceID)\" -ErrorAction SilentlyContinue
                if ($content) {
                    Write-Host "   $($_.DeviceID) - Accessibile ✅" -ForegroundColor Green
                } else {
                    Write-Host "   $($_.DeviceID) - Non accessibile ❌" -ForegroundColor Red
                }
            } catch {
                Write-Host "   $($_.DeviceID) - Errore di accesso ⚠️" -ForegroundColor Yellow
            }
        }
    }
    
    # Info sui log
    if (Test-Path $LOG_FILE) {
        $logSize = (Get-Item $LOG_FILE).Length
        Write-Host "📋 Log file: $LOG_FILE ($([math]::Round($logSize/1KB,1))KB)" -ForegroundColor Cyan
    } else {
        Write-Host "📋 Log file: Non trovato" -ForegroundColor Yellow
    }
}

function Show-Logs {
    param([int]$Lines = 20)
    
    if (Test-Path $LOG_FILE) {
        Write-Host "📋 Ultimi $Lines righe del log:" -ForegroundColor Cyan
        Write-Host "─────────────────────────────" -ForegroundColor Cyan
        Get-Content $LOG_FILE -Tail $Lines
    } else {
        Write-Host "❌ File di log non trovato: $LOG_FILE" -ForegroundColor Red
    }
}

function Restart-RemoteFs {
    Write-Host "🔄 Riavvio RemoteFS..." -ForegroundColor Yellow
    Stop-RemoteFs
    Start-Sleep 2
    Start-RemoteFs
}

function Run-Tests {
    Write-Host "🧪 Esecuzione test RemoteFS Windows..." -ForegroundColor Blue
    
    # Salva la directory corrente
    $originalLocation = Get-Location
    
    # Percorso della directory tests
    $testsDir = Join-Path $projectRoot "tests"
    $testScript = Join-Path $testsDir "test_windows_backend.ps1"
    
    if (-not (Test-Path $testsDir)) {
        Write-Host "❌ Directory tests non trovata: $testsDir" -ForegroundColor Red
        return
    }
    
    if (-not (Test-Path $testScript)) {
        Write-Host "❌ Script di test non trovato: $testScript" -ForegroundColor Red
        return
    }
    
    Write-Host "ℹ️  Avvio script di test: $testScript" -ForegroundColor Yellow
    Write-Host "─────────────────────────────────────────" -ForegroundColor Cyan
    
    # Esegui lo script di test
    try {
        Set-Location $testsDir
        & ".\test_windows_backend.ps1"
        
        if ($LASTEXITCODE -eq 0) {
            Write-Host "✅ Test completati con successo!" -ForegroundColor Green
        } else {
            Write-Host "❌ Test falliti" -ForegroundColor Red
        }
    }
    catch {
        Write-Host "❌ Errore durante l'esecuzione dei test: $($_.Exception.Message)" -ForegroundColor Red
    }
    finally {
        # Ripristina sempre la directory originale
        Set-Location $originalLocation
    }
}

# Main logic
switch ($Action.ToLower()) {
    "start" { Start-RemoteFs }
    "stop" { Stop-RemoteFs }
    "restart" { Restart-RemoteFs }
    "status" { Show-Status }
    "logs" { Show-Logs }
    "shell" { Open-NewShellForDrive }
    "test" { Run-Tests }
    "help" {
        Write-Host @"
RemoteFS Client Control Script

Usage: .\remote-fs-ctl.ps1 [action]

Actions:
  start     - Avvia il daemon RemoteFS
  stop      - Ferma il daemon RemoteFS  
  restart   - Riavvia il daemon
  status    - Mostra stato dei processi e mount
  logs      - Mostra ultimi log
  shell     - Apre nuova finestra PowerShell sul drive R:
  test      - Esegue i test del filesystem Windows
  help      - Mostra questo help

Examples:
  .\remote-fs-ctl.ps1 start
  .\remote-fs-ctl.ps1 status
  .\remote-fs-ctl.ps1 test
  .\remote-fs-ctl.ps1 shell
  .\remote-fs-ctl.ps1 stop
"@ -ForegroundColor White
    }
    default {
        Write-Host "❌ Azione non riconosciuta: $Action" -ForegroundColor Red
        Write-Host "Usa 'help' per vedere le azioni disponibili" -ForegroundColor Yellow
    }
}
