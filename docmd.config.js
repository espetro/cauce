// docmd (docmd.io) config for the cauce docs site, served at
// https://docs.cauce.fyi via deploy/cf-pages/build-docs.sh on the
// cauce-docs Pages project (docs live in the public repo; apex/api
// deploys belong to cauce-instance).
// srcDir holds END-USER docs only (install, usage, deploy guides).
// Developer docs live in .agents/docs and are copied into
// docs/developers/ at build time — docmd doesn't follow symlinks.
module.exports = {
  siteTitle: "cauce docs",
  siteUrl: "https://docs.cauce.fyi",
  srcDir: "docs",
  outputDir: "docs-site",
  minify: true,
  autoTitleFromH1: true,
  copyCode: true,
  pageNavigation: true,
  favicon: "assets/favicon.ico",
  logo: {
    // Neutral mark — the same image reads on both themes.
    light: "assets/images/cauce-mark-64.png",
    dark: "assets/images/cauce-mark-64.png",
    href: "/",
    alt: "cauce",
    height: "26px",
  },
  versions: {
    // Versioned doc trees. `current` is the source of truth for this
    // release (docs/); archived trees live in docs-versions/<id>/ and
    // are staged in by deploy/cf-pages/build-docs.sh from git tags.
    current: "0.9",
    all: [
      { id: "0.8", dir: "docs-versions/0.8", label: "v0.8.1" },
    ],
  },
  plugins: {
    // Docmd's LLM chat widget is off; the static llms.txt/okf bundles
    // the build already ships are the agent surface we support.
    ai: { assistant: false },
  },
};
