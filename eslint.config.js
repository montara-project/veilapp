// oxlint (see .oxlintrc.json) covers correctness rules. ESLint runs solely
// for eslint-plugin-perfectionist, which oxlint has not ported yet;
// typescript-eslint supplies the parser so ESLint can read TS syntax.
import perfectionist from 'eslint-plugin-perfectionist'
import tseslint from 'typescript-eslint'

export default [
  {
    ignores: ['dist/**', 'src-tauri/**'],
  },
  {
    files: ['**/*.ts'],
    languageOptions: { parser: tseslint.parser },
    plugins: { perfectionist },
    rules: {
      'perfectionist/sort-imports': [
        'error',
        {
          type: 'alphabetical',
          ignoreCase: true,
        },
      ],
    },
  },
]
