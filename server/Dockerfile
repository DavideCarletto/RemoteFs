# Dockerfile per il server RemoteFS
FROM node:22-alpine

# Installa dipendenze di sistema
RUN apk add --no-cache \
    sqlite \
    curl

# Crea directory di lavoro
WORKDIR /app

# Copia package.json e installa dipendenze
COPY package*.json ./
RUN npm install

# Copia il codice sorgente
COPY src/ ./src/
COPY tsconfig.json ./

# Compila TypeScript
RUN npm run build

# Crea directory per il database
RUN mkdir -p /app/data

# Espone la porta
EXPOSE 3000

# Comando di avvio
CMD ["npm", "start"]
