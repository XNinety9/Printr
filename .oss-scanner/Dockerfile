# Image d'analyse pour OSS Scanner (https://github.com/anthropics/oss-scanner) : construite avec le dépôt
# comme contexte, avec Internet ; l'analyse se fait ensuite sans réseau, donc tout ce que les tests
# demandent (dépendances, listes de mots) est récupéré ici.
FROM rust:1-bookworm

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_TERM_COLOR=never \
    CARGO_INCREMENTAL=0 \
    RUSTFLAGS="-C force-frame-pointers=yes"

RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

# Barnum (horoscope hors ligne), lancé par Printr comme commande externe : au même commit que l'image
# de deploy/Dockerfile.
ARG BARNUM_COMMIT=203a36b664eb5cba4c5fc5e5924818bcc4a36a96
RUN git clone --quiet https://github.com/XNinety9/Barnum /opt/barnum \
    && git -C /opt/barnum checkout --quiet "$BARNUM_COMMIT"
ENV PRINTR_BARNUM="python3 /opt/barnum/main.py"

# Contrat du scanner : le dépôt dans l'image, en /src.
COPY . /src
WORKDIR /src

# Dépendances, puis compilation avec les infos de débogage (profil dev) : le binaire et les tests.
RUN cargo fetch --locked
RUN cargo build --locked --all-targets

# Les tests une première fois, en ligne : ils remplissent les caches (listes de mots des grilles) pour
# les relancer hors ligne. Un test en échec n'arrête pas la construction.
RUN cargo test --locked || echo "WARNING: cargo test reported failures"
