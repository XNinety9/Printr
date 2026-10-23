# Printr en conteneur (Docker, Podman), à lancer avec compose.yaml : voir le guide (docs/guide.fr.md), « En conteneur (Docker) ».
# Le binaire `printr` doit être à côté de ce fichier : celui d'une archive de release (déjà là une
# fois l'archive décompressée), ou un binaire compilé pour la machine (cross, scripts/deploy.sh).
FROM debian:bookworm-slim

# Barnum (horoscope hors ligne) : du Python sans dépendance, à un commit figé.
ARG BARNUM_COMMIT=203a36b664eb5cba4c5fc5e5924818bcc4a36a96
ADD https://github.com/XNinety9/Barnum/archive/${BARNUM_COMMIT}.tar.gz /tmp/barnum.tar.gz

# tzdata : l'heure locale (TZ) des planifications ; ca-certificates : TLS vers les API et MQTT.
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 tzdata ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir /opt/barnum \
    && tar -xzf /tmp/barnum.tar.gz -C /opt/barnum --strip-components=1 \
    && rm /tmp/barnum.tar.gz \
    && python3 -m compileall -q /opt/barnum \
    && groupadd --system --gid 10001 printr \
    && useradd --system --uid 10001 --gid printr --home-dir /var/lib/printr --no-create-home \
        --shell /usr/sbin/nologin printr \
    && install -d -o printr -g printr -m 0770 /var/lib/printr /var/cache/printr

COPY --chmod=755 printr /usr/local/bin/printr

# Valeurs propres au conteneur ; le reste vient de printr.env (voir compose.yaml).
ENV PRINTR_DATA_DIR=/var/lib/printr \
    PRINTR_CACHE_DIR=/var/cache/printr \
    PRINTR_LISTEN=0.0.0.0:8080 \
    PRINTR_DEVICE=/host/dev/tm88 \
    PRINTR_BARNUM="python3 /opt/barnum/main.py"

USER printr
EXPOSE 8080
ENTRYPOINT ["printr"]
CMD ["serve"]
