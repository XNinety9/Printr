"""Captures d'écran de l'interface web pour le site et le README (données fictives).

Lancé par photo-session.sh, qui démarre un Printr temporaire : python capture.py URL DOSSIER
"""

import asyncio
import sys

from playwright.async_api import async_playwright

URL, OUT = sys.argv[1], sys.argv[2]
PHOTO = sys.argv[3]


async def login(page, name, password="demo1234"):
    await page.goto(URL)
    await page.get_by_text(name, exact=True).first.click()
    await page.fill("input[type=password]", password)
    await page.click("button:has-text('Entrer')")
    await page.wait_for_selector("text=Mes tickets")


async def open_preset(page, name):
    card = page.locator(".preset", has_text=name)
    await card.get_by_role("button", name="Modifier").click()
    await page.wait_for_timeout(1500)


async def mobile(p):
    browser = await p.chromium.launch(executable_path="/usr/bin/chromium")
    ctx = await browser.new_context(viewport={"width": 390, "height": 844}, device_scale_factor=2,
                                    is_mobile=True, has_touch=True, locale="fr-FR", color_scheme="light")
    page = await ctx.new_page()
    await page.goto(URL)
    await page.wait_for_timeout(500)
    await page.get_by_text("Alex", exact=True).first.click()
    await page.wait_for_timeout(300)
    await page.screenshot(path=f"{OUT}/mobile-login.png")
    await login(page, "Alex")
    await page.wait_for_timeout(600)
    await page.screenshot(path=f"{OUT}/mobile-home.png")

    await open_preset(page, "Pause jeux")
    await page.get_by_role("button", name="Aperçu").first.click()
    await page.wait_for_timeout(1500)
    await page.screenshot(path=f"{OUT}/mobile-preview.png")
    await page.keyboard.press("Escape")

    await page.goto(URL + "/#/compose")
    await page.wait_for_timeout(400)
    await page.locator(".block-head").nth(1).click()
    await page.wait_for_timeout(400)
    await page.screenshot(path=f"{OUT}/mobile-compose.png")

    await page.goto(URL + "/#/message")
    await page.set_input_files("input[type=file]", PHOTO)
    await page.wait_for_timeout(1500)
    await page.fill("textarea", "Un petit café pour bien commencer la journée. Je pense fort à toi !")
    await page.wait_for_timeout(1500)
    await page.screenshot(path=f"{OUT}/mobile-message.png")
    await browser.close()


async def desktop(p, scheme):
    browser = await p.chromium.launch(executable_path="/usr/bin/chromium")
    ctx = await browser.new_context(viewport={"width": 1440, "height": 900}, device_scale_factor=2,
                                    locale="fr-FR", color_scheme=scheme)
    page = await ctx.new_page()
    await login(page, "Alex")
    await page.wait_for_timeout(600)
    await page.screenshot(path=f"{OUT}/desktop-home-{scheme}.png")
    await open_preset(page, "Pause jeux")
    await page.screenshot(path=f"{OUT}/desktop-compose-{scheme}.png")
    await page.click("text=Ajouter un bloc")
    await page.wait_for_timeout(600)
    await page.screenshot(path=f"{OUT}/desktop-catalog-{scheme}.png")
    await browser.close()


async def main():
    async with async_playwright() as p:
        await mobile(p)
        await desktop(p, "light")
        await desktop(p, "dark")


asyncio.run(main())
