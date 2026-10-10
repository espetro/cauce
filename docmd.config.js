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
};
