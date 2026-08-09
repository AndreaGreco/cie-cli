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
					label: 'Guida',
					items: [
						{ label: 'Installazione', slug: 'guida/installazione' },
						{ label: 'Primi passi', slug: 'guida/primi-passi' },
						{ label: 'Il PIN', slug: 'guida/pin' },
						{ label: 'Firmare documenti', slug: 'guida/firma' },
						{ label: 'Certificare una chiave OpenPGP', slug: 'guida/openpgp' },
						{ label: 'Interfaccia grafica', slug: 'guida/interfaccia-grafica' },
					],
				},
				{
					label: 'Riferimento',
					items: [
						{ label: 'Comandi', slug: 'riferimento/comandi' },
						{ label: 'Specifica della certification', slug: 'riferimento/specifica-openpgp' },
						{ label: 'Cosa diventa pubblico', slug: 'riferimento/privacy' },
						{ label: 'Architettura', slug: 'riferimento/architettura' },
					],
				},
				{
					label: 'Progetto',
					items: [
						{ label: 'Limiti e stato', slug: 'progetto/limiti' },
						{ label: 'Sviluppo', slug: 'progetto/sviluppo' },
					],
				},
			],
		}),
	],
});
