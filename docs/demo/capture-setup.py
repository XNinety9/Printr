"""Captures de la première mise en route (données fictives) : choix du Wi-Fi depuis le point
d'accès, assistant de premier lancement, réglages.

Lancé par setup-session.sh, dans son réseau isolé : python capture-setup.py DOSSIER
- http://localhost/ : un Printr en point d'accès (PRINTR_WIFI_SETUP=1) ;
- http://printr.local/ : un Printr neuf, pour le premier compte, l'assistant et les réglages.
"""

import asyncio
import os
import sys

from playwright.async_api import async_playwright

OUT = sys.argv[1]
EN = os.environ.get("DEMO_LANG", "fr") == "en"
W = {
    "home_wifi": "Home Sweet Home" if EN else "La Maison",
    "create": "Create my account" if EN else "Créer mon compte",
    "test": "Print a test ticket" if EN else "Imprimer un ticket de test",
    "yes": "Yes, perfect" if EN else "Oui, parfait",
    "next": "Next" if EN else "Suivant",
    "city": "Boston" if EN else "Bordeaux",
    "go": "Let’s go" if EN else "C’est parti",
    "settings": "Settings" if EN else "Réglages",
    "locale": "en-US" if EN else "fr-FR",
}


async def snap(page, name):
    """Capture d'écran, annoncée sur la sortie (scripts/illustrations.sh affiche la progression)."""
    path = f"{OUT}/{name}.png"
    await page.screenshot(path=path)
    print(path, flush=True)


async def wifi(page):
    # Le point d'accès s'ouvre après la minute d'attente du réseau.
    await page.goto("http://localhost/")
    for _ in range(120):
        status = await page.evaluate("fetch('/api/wifi').then((r) => r.status)")
        if status == 200:
            break
        await page.wait_for_timeout(1000)
    else:
        raise SystemExit("le point d'accès ne s'est pas ouvert")
    await page.goto("http://localhost/")
    await page.get_by_text(W["home_wifi"], exact=True).click()
    await page.fill("input[type=password]", "motdepasse")
    await page.wait_for_timeout(300)
    await snap(page, "mobile-wifi")


async def wizard(page):
    await page.goto("http://printr.local/")
    await page.fill("input[autocomplete=username]", "Camille")
    await page.fill("input[type=password]", "demo1234")
    await page.click(f"button:has-text('{W['create']}')")
    # Détection des imprimantes (USB, puis le réseau sur le port 9100).
    await page.click(f"button:has-text('{W['test']}')", timeout=30000)
    await page.wait_for_selector(f"button:has-text('{W['yes']}')")
    await page.wait_for_timeout(300)
    await snap(page, "mobile-wizard")
    await page.click(f"button:has-text('{W['yes']}')")
    await page.click(f"button:has-text('{W['next']}')")
    await page.fill(".wizard input", W["city"])
    await page.click(f"button:has-text('{W['next']}')")
    await page.click(f"button:has-text('{W['next']}')")
    # Les réglages s'appliquent par un redémarrage : la page se recharge toute seule.
    await page.click(f"button:has-text('{W['go']}')")
    # Sur téléphone, « Mon compte » est dans la barre du haut (celui de la barre latérale est masqué).
    await page.wait_for_selector("header button.me", timeout=60000)
    await page.wait_for_timeout(800)
    await page.click("header button.me")
    await page.click(f"button:has-text('{W['settings']}')")
    await page.wait_for_timeout(1200)
    await snap(page, "mobile-settings")


async def main():
    async with async_playwright() as p:
        browser = await p.chromium.launch(
            executable_path="/usr/bin/chromium",
            args=["--host-resolver-rules=MAP printr.local 192.168.1.10"],
        )
        ctx = await browser.new_context(viewport={"width": 390, "height": 844}, device_scale_factor=2,
                                        is_mobile=True, has_touch=True, locale=W["locale"], color_scheme="light")
        page = await ctx.new_page()
        await wifi(page)
        await wizard(page)
        await browser.close()


asyncio.run(main())
