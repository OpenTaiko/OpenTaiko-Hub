<script>
    // Renders the documentation shipped inside the selected OpenTaiko instance
    // (<instance>/docs/index.html) entirely locally: the page and every resource it
    // references (stylesheets, scripts, images) are read from disk and inlined into a
    // single self-contained document displayed through the iframe's srcdoc. No
    // requests of any kind are made — the file is read and rendered as is.
    import { exists, readTextFile, readFile, readDir } from '@tauri-apps/plugin-fs';
    import { path } from '@tauri-apps/api';
    import { _ } from 'svelte-i18n';

    import { activeInstance } from "../stores/instances.js";

    let docsHtml = null;
    let notFound = false;
    let loading = false;
    let refreshToken = 0;
    let renderedFor = null;

    // Only (re)build when the active instance actually changes — building inlines the
    // whole docs site, so avoid redundant rebuilds on unrelated store updates.
    $: if ($activeInstance && renderedFor !== $activeInstance.id) {
        renderedFor = $activeInstance.id;
        Refresh($activeInstance);
    }

    const IMAGE_MIME = {
        png: 'image/png',
        jpg: 'image/jpeg',
        jpeg: 'image/jpeg',
        gif: 'image/gif',
        svg: 'image/svg+xml',
        ico: 'image/x-icon',
        webp: 'image/webp'
    };

    const toBase64 = (bytes) => {
        let binary = '';
        const chunkSize = 0x8000;
        for (let i = 0; i < bytes.length; i += chunkSize) {
            binary += String.fromCharCode(...bytes.subarray(i, i + chunkSize));
        }
        return btoa(binary);
    };

    // Reads a file referenced by index.html; only plain relative child paths are accepted
    const readDocsAsset = async (docsDir, relPath) => {
        if (relPath.includes('..') || relPath.startsWith('/') || /^[a-z]+:/i.test(relPath)) return null;
        try {
            // path.join normalizes forward slashes for the current platform
            return await readTextFile(await path.join(docsDir, relPath));
        } catch {
            return null;
        }
    };

    const BuildInlinedDocs = async (docsDir) => {
        const indexPath = await path.join(docsDir, 'index.html');
        let html = await readTextFile(indexPath);

        // Tag strings are assembled dynamically: literal style/script tags in this
        // source would break Svelte's regex-based preprocessing of this component.
        const STYLE_OPEN = '<' + 'style>';
        const STYLE_CLOSE = '</' + 'style>';

        // Inline stylesheets
        const linkTags = [...html.matchAll(/<link\b[^>]*>/gi)];
        for (const [tag] of linkTags) {
            const href = tag.match(/href="([^"]+)"/i)?.[1];
            if (!href) continue;
            if (/rel="stylesheet"/i.test(tag)) {
                const css = await readDocsAsset(docsDir, href);
                html = html.replace(tag, css !== null ? `${STYLE_OPEN}\n${css}\n${STYLE_CLOSE}` : '');
            } else if (/rel="icon"/i.test(tag)) {
                html = html.replace(tag, '');
            }
        }

        // Inline scripts (in document order)
        const SCRIPT_OPEN = '<' + 'script>';
        const SCRIPT_CLOSE = '</' + 'script>';
        const scriptTags = [...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"[^>]*><\/script>/gi)];
        for (const [tag, src] of scriptTags) {
            const js = await readDocsAsset(docsDir, src);
            if (js !== null) {
                // "</script" inside inlined code would end the tag early; escape it
                // (only occurs inside string literals, where "<\/script" is equivalent)
                const safe = js.replace(/<\/script/gi, '<\\/script');
                html = html.replace(tag, `${SCRIPT_OPEN}\n${safe}\n${SCRIPT_CLOSE}`);
            } else {
                html = html.replace(tag, '');
            }
        }

        // Embed the media folder as data URIs and fix rendered <img> tags pointing at it
        const mediaMap = {};
        try {
            const mediaDir = await path.join(docsDir, 'media');
            for (const entry of await readDir(mediaDir)) {
                if (!entry.isFile) continue;
                const ext = entry.name.split('.').pop()?.toLowerCase();
                const mime = IMAGE_MIME[ext];
                if (!mime) continue;
                const bytes = await readFile(await path.join(mediaDir, entry.name));
                mediaMap[entry.name] = `data:${mime};base64,${toBase64(bytes)}`;
            }
        } catch {
            // No media folder: nothing to embed
        }

        const mediaFixer = `${SCRIPT_OPEN}
(function () {
    var MEDIA = ${JSON.stringify(mediaMap)};
    function fix(root) {
        var imgs = root.querySelectorAll ? root.querySelectorAll('img') : [];
        for (var i = 0; i < imgs.length; i++) {
            var src = imgs[i].getAttribute('src') || '';
            var match = src.match(/^(?:\\.\\/)?media\\/(.+)$/);
            if (match && MEDIA[match[1]]) imgs[i].src = MEDIA[match[1]];
        }
    }
    new MutationObserver(function () { fix(document); }).observe(document.documentElement, { childList: true, subtree: true });
    document.addEventListener('DOMContentLoaded', function () { fix(document); });
})();
${SCRIPT_CLOSE}`;
        html = html.includes('</body>') ? html.replace('</body>', `${mediaFixer}\n</body>`) : html + mediaFixer;

        return html;
    };

    const Refresh = async (instance) => {
        const token = ++refreshToken;
        docsHtml = null;
        notFound = false;
        loading = true;
        try {
            const docsDir = await path.join(instance.path, 'docs');
            const indexPath = await path.join(docsDir, 'index.html');
            if (!(await exists(indexPath))) {
                if (token === refreshToken) {
                    notFound = true;
                    loading = false;
                }
                return;
            }
            const html = await BuildInlinedDocs(docsDir);
            if (token === refreshToken) {
                docsHtml = html;
                loading = false;
            }
        } catch (error) {
            console.error('Failed to render the docs:', error);
            if (token === refreshToken) {
                notFound = true;
                loading = false;
            }
        }
    };
</script>

{#if notFound}
    <section class="card w-full">
        <div class="p-4 space-y-4">
            <p><b>{$_('docs.not_found')}</b></p>
            <p class="opacity-70">{$_('docs.not_found_hint')}</p>
        </div>
    </section>
{:else if docsHtml !== null}
    <iframe srcdoc={docsHtml} title={$_('docs.title')} class="docs-frame"></iframe>
{:else if loading}
    <div class="placeholder animate-pulse w-full h-24" />
{/if}

<style>
    .docs-frame {
        width: 100%;
        height: 100%;
        min-height: 75vh;
        border: none;
        border-radius: 0.5rem;
        background: #fff;
    }
</style>
