// esbuild bundles the Typst package's files as text (`--loader:.typ=text`).
declare module "*.typ" {
  const text: string;
  export default text;
}
declare module "*.toml" {
  const text: string;
  export default text;
}
