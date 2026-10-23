# Security policy

## Supported versions

Security fixes go into the latest release only. Printr checks GitHub for new versions every day and
tells the administrator in the app, so please update to the latest release before reporting.

## Reporting a vulnerability

Please **do not open a public issue** for a security problem. Instead, email **x99@x99.fr** with:

- what an attacker can do, and what they need first (an account, the API token, access to the MQTT
  broker, nothing at all…);
- the version (`printr --version`) and how Printr is run (systemd service, container, behind a reverse
  proxy or not);
- the steps or a minimal ticket / request that reproduces it, and a patch if you have one.

Reports in English or French are welcome. Printr is maintained on my free time: I will acknowledge
your report within a week, and keep you informed until a fix is released. Please give me a reasonable
time to release it before disclosing the issue publicly; I am happy to credit you in the changelog.

## Scope

What counts as a vulnerability, how severity is rated and what is out of scope (for example, MQTT
accepting prints from any client allowed on the broker, by design) is described in the
[threat model](.oss-scanner/threat_model.md).
