// docmd (docmd.io) config for the cauce docs site, served at
// https://cauce.fyi/docs via deploy/cf-pages/build.sh.
// srcDir holds END-USER docs only (install, usage, deploy guides).
// Developer docs live in .agents/docs and are copied into
// docs/developers/ at build time — docmd doesn't follow symlinks.
module.exports = {
  siteTitle: "cauce docs",
  siteUrl: "https://cauce.fyi",
  srcDir: "docs",
  outputDir: "docs-site",
  minify: true,
  autoTitleFromH1: true,
  copyCode: true,
  pageNavigation: true,
};
