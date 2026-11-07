# Printr en conteneur (Docker, Podman), à lancer avec compose.yaml : voir le guide (docs/guide.fr.md), « En conteneur (Docker) ».
# Le binaire `printr` doit être à côté de ce fichier : celui d'une archive de release (déjà là une
# fois l'archive décompressée), ou un binaire compilé pour la machine (cross, scripts/deploy.sh).
FROM debian:bookworm-slim

# Barnum (horoscope hors ligne) : du Python sans dépendance, à un commit figé.
ARG BARNUM_COMMIT=203a36b664eb5cba4c5fc5e5924818bcc4a36a96
ADD https://github.com/XNinety9/Barnum/archive/${BARNUM_COMMIT}.tar.gz /tmp/barnum.tar.gz

# Vetuste Scripteur (vieux françois) : un seul fichier Python, à une version figée et vérifiée.
ARG VETUSTE_VERSION=1.1.1
ARG VETUSTE_SHA256=8de37c08af710c495b30085e1b8a24fbed63576904ed4b62c6ee907a1a8ed108
ADD --checksum=sha256:${VETUSTE_SHA256} --chmod=644 \
    https://github.com/XNinety9/vetuste_scripteur/releases/download/v${VETUSTE_VERSION}/vetuste-scripteur.pyz \
    /opt/vetuste/vetuste-scripteur.pyz

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
    PRINTR_BARNUM="python3 /opt/barnum/main.py" \
    PRINTR_VETUSTE="python3 /opt/vetuste/vetuste-scripteur.pyz"

USER printr
EXPOSE 8080
ENTRYPOINT ["printr"]
CMD ["serve"]
