export async function resolve(specifier, context, defaultResolve) {
  if (specifier === 'dompurify') {
    return { url: 'dompurify:stub', shortCircuit: true };
  }
  return defaultResolve(specifier, context, defaultResolve);
}

export async function load(url, context, defaultLoad) {
  if (url === 'dompurify:stub') {
    const source = `export default {
  sanitize: (input) => input,
  addHook: () => {},
  removeHook: () => {},
};`;
    return { format: 'module', source, shortCircuit: true };
  }
  return defaultLoad(url, context, defaultLoad);
}
