import { defineRouteMiddleware } from '@astrojs/starlight/route-data';

// Il cartello "in sviluppo" sta su ogni pagina, e va tolto da qui - un punto
// solo - quando esce la 1.0.0. Il `??=` lascia vincere un banner scritto nel
// frontmatter di una pagina specifica.
export const onRequest = defineRouteMiddleware((context) => {
	context.locals.starlightRoute.entry.data.banner ??= {
		content:
			'<strong>Progetto in sviluppo.</strong> Non è garantito che funzioni, e le cose possono cambiare da una versione all\'altra. ' +
			'La 1.0.0 non è ancora uscita: <a href="https://github.com/AttilioGreco/cie-cli/releases">tieni d\'occhio le release</a>.',
	};
});
