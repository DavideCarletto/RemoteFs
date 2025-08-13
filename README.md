Ciao Michi

Per ora funziona tutto su linux (fuse funziona solo con linux), io sto lavorando da windows e uso wsl. 

**Prerequisiti**
- WSL installato (consigliato Ubuntu)
- Node.js e npm installati in WSL (`sudo apt install nodejs npm`)
- Rust installato in WSL (`curl https://sh.rustup.rs -sSf | sh`)
- FUSE installato in WSL (`sudo apt-get install fuse3 libfuse3-dev pkg-config`)
  
Dopo aver scaricato fuse, esegui il seguente comando:

`sudo sed -i 's/^#user_allow_other/user_allow_other/' /etc/fuse.conf`

questo serve per modificare una riga di fuse.conf che permette di fare l'unmounting automatico (serve che la riga con user_allow_other sia decommmentata)

Poi se vuoi testare che il client e il server funzionino devi fare:

**1. Clona il repository**
```sh
git clone https://github.com/DavideCarletto/RemoteFs.git
cd remote_fs
```

**2. Installa le dipendenze del server**
```sh
cd server
npm install
```

**3. Avvia il server**
```sh
ts-node src/server.ts
```
Il server Express sarà attivo su `localhost:3000` o la porta configurata. Se lavori su windows e vuoi far partire il server non usare npm start ma usa il comando qua sopra (da wsl), altrimenti fa casino e vscode ti fa partire il server su windows e non riuscirà a comunicare con il client (che parte da shell wsl)

**4. Installa le dipendenze del client Rust**
```sh
cd ../client
cargo build
```

**5. Avvia il client**
```sh
cargo run
```
(Il client Rust monterà il filesystem remoto.)

---

**Note**
- Tutti i comandi vanno eseguiti in una shell WSL.


**TODO**
- Una volta finito di sviluppare, eliminare possibilità di scegliere se daemon o no (dovrebbe partire a prescindere daemon)
- Per il logging di fern, a fine sviluppo cambiare da truncate ad append
- Nn main, mettere .level(log::LevelFilter::Debug) a info una volta finito