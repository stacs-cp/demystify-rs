serve: website/puzzle.js
	python -m http.server -d website

website/puzzle.js: puzzle.tsx
	npx esbuild puzzle.tsx --bundle --sourcemap --outdir=website

update:
	npx esbuild puzzle.tsx --bundle --sourcemap --outdir=website --watch