// This is the configuration file for Prettier, the auto-formatter:
// https://prettier.io/docs/en/configuration.html

/** @type {import("prettier").Config} */
const config = {
  overrides: [
    // Allow proper formatting of JSONC files that have JSON file extensions.
    {
      files: ["**/.vscode/*.json", "**/tsconfig.*.json", "**/tsconfig.json"],
      options: {
        parser: "jsonc",
      },
    },

    // By default, Prettier will not break long lines in Markdown files:
    // https://prettier.io/docs/options#prose-wrap
    // We only want this setting to apply to Markdown files because it causes weird glitches in YAML
    // files.
    {
      files: ["**/*.md"],
      options: {
        proseWrap: "always",
      },
    },
  ],
};

export default config;
