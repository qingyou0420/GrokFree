import reactHooks from "eslint-plugin-react-hooks";
import tsParser from "@typescript-eslint/parser";

/**
 * 自用版：只挂 react-hooks 两条规则（审查报告 2026-08-16 P0-2）。
 * 不上全量规则集——那是防多人风格漂移的，单人项目装了只添噪音。
 */
export default [
  {
    ignores: ["dist/**", "node_modules/**", "src-tauri/**", "scripts/**"],
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: "module",
        ecmaFeatures: { jsx: true },
      },
    },
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
    },
  },
];
