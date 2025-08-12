import { hubdefault } from './hubdefault'
import { dashy } from './dashy'
import { deceiver } from './deceiver'
import { onyx } from './onyx'

// @ts-check
import { join } from 'path';

// 1. Import the Skeleton plugin
import { skeleton } from '@skeletonlabs/tw-plugin';

/** @type {import('tailwindcss').Config} */
export default {
	// 2. Opt for dark mode to be handled via the class method
	darkMode: 'class',
	content: [
		'./src/**/*.{html,js,svelte,ts}',
		// 3. Append the path to the Skeleton package
		join(require.resolve(
			'@skeletonlabs/skeleton'),
			'../**/*.{html,js,svelte,ts}'
		)
	],
	safelist: [
		{
		  pattern: /bg-(blue|green|yellow|red|purple|orange|blue)-100/,
		},
		{
		  pattern: /text-(blue|green|yellow|red|purple|orange|blue)-800/,
		}
	],
	theme: {
		extend: {},
	},
	plugins: [
		skeleton({
            themes: { preset: [ "skeleton", "crimson", "gold-nouveau", "hamlindigo", "modern", "rocket", "sahara", "seafoam", "vintage", "wintry" ],
                custom: [ hubdefault, dashy, deceiver, onyx ]
            }
        })
	]
}
