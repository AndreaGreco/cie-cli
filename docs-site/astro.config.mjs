// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Il sito vive sulle Pages del progetto, quindi sotto /cie-cli/: senza `base`
// ogni link interno punterebbe alla radice del dominio utente.
export default defineConfig({
	site: 'https://attiliogreco.github.io',
	base: '/cie-cli',
	integrations: [
		starlight({
			title: 'CIE-CLI',
			description:
				"CLI e libreria Rust per la Carta d'Identita' Elettronica italiana, in PC/SC diretto: firma di documenti e certificazione OpenPGP senza middleware.",
			defaultLocale: 'root',
			locales: {
				root: { label: 'Italiano', lang: 'it' },
			},
			customCss: ['./src/styles/theme.css'],
			// Il banner "in sviluppo" su tutte le pagine, scritto una volta sola.
			routeMiddleware: './src/routeData.ts',
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/AttilioGreco/cie-cli',
				},
			],
			editLink: {
				baseUrl: 'https://github.com/AttilioGreco/cie-cli/edit/main/docs-site/',
			},
			sidebar: [
				{
					label: 'Usare Firma CIE',
					items: [
						{ label: "Cos'e' e cosa puoi farci", slug: 'guida/cosa-fa' },
						{ label: 'Installare', slug: 'guida/installazione' },
						{ label: 'Firmare un documento', slug: 'guida/firmare' },
						{ label: 'Il PIN', slug: 'guida/pin' },
						{ label: 'Se qualcosa non va', slug: 'guida/problemi' },
					],
				},
				{
					label: 'Riga di comando',
					items: [
						{ label: 'Primi passi', slug: 'riferimento/primi-passi' },
						{ label: 'Tutti i comandi', slug: 'riferimento/comandi' },
						{ label: 'Certificare una chiave OpenPGP', slug: 'riferimento/openpgp' },
						{ label: 'Cosa diventa pubblico', slug: 'riferimento/privacy' },
					],
				},
				{
					label: 'Per sviluppatori',
					items: [
						{ label: 'Architettura e protocollo', slug: 'progetto/architettura' },
						{ label: 'Specifica della certification', slug: 'progetto/specifica-openpgp' },
						{ label: 'Compilare e testare', slug: 'progetto/sviluppo' },
						{ label: 'Limiti e stato', slug: 'progetto/limiti' },
					],
				},
			],
		}),
	],
});
