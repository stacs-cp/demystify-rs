serve: website/puzzle-bundle.js
	python -m http.server

website/puzzle-bundle.js: website/puzzle.tsx
	npx esbuild website/puzzle.tsx --bundle --sourcemap > website/puzzle-bundle.js