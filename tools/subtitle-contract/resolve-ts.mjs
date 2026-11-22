// Module resolve hook for loading the Subtitle extension's sources unchanged: they import their
// siblings without an extension (`./api`, as Vite resolves them); Node needs `./api.ts`.
export async function resolve(specifier, context, next) {
  try {
    return await next(specifier, context);
  } catch (e) {
    const relative = specifier.startsWith('./') || specifier.startsWith('../');
    if (relative && !/\.[cm]?[jt]s$/.test(specifier) && e?.code === 'ERR_MODULE_NOT_FOUND') {
      return next(`${specifier}.ts`, context);
    }
    throw e;
  }
}
