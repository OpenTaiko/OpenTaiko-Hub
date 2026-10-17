/// <reference types="vitest/config" />
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	// Unit tests cover the modules that run without the Tauri backend (npm test)
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node',
	},
	resolve: {
		dedupe: ['svelte'],
	},
	server: {
		// Dedicated port for the Hub: never share (or silently fall back from) the
		// common 5173, so the webview can never attach to an unrelated dev server.
		port: 15731,
		strictPort: true,
		watch: {
			// Never watch the Rust build tree: in dev mode the Hub's working data
			// (downloads, experimental build sources) lives under src-tauri/target,
			// and watching it crashes the dev server (EBUSY) during builds.
			// dev-hub holds the dev build's instances and songs (devdata.rs), same reason.
			ignored: ['**/src-tauri/**', '**/dev-hub/**'],
		},
	},
});
