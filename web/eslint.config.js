// ESLint settings: the recommended JavaScript and TypeScript rules.
import js from '@eslint/js';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  { ignores: ['dist/', 'src/wasm/', 'test-results/', 'playwright-report/'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    // Build scripts run in Node, not the browser.
    files: ['scripts/**/*.mjs', 'tests-node/**/*.mjs'],
    languageOptions: {
      globals: { process: 'readonly', console: 'readonly', URL: 'readonly' },
    },
  },
);
