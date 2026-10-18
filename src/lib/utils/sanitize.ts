import DOMPurify from 'dompurify';

// Everything rendered with {@html} goes through here. Some of that markup comes from
// outside the Hub (changelogs fetched from GitHub, a file name quoted in an error), and
// script running in this page can call the backend. Formatting tags survive; scripts,
// event handlers and javascript: links do not. Neither do styles and form controls: a
// single changelog line could otherwise lay a look-alike of the Hub, or a fake form, over
// the whole window.
const FORBID_TAGS = ['style', 'form', 'input', 'button', 'textarea', 'select', 'option'];

// Always supported in the webview; not in the unit tests, which run without a DOM
if (DOMPurify.isSupported) {
    DOMPurify.addHook('afterSanitizeAttributes', (node) => {
        // A link opened from here gets no handle back on the Hub
        if (node.tagName === 'A' && node.hasAttribute('href')) {
            node.setAttribute('rel', 'noopener noreferrer');
        }
    });
}

export const SafeHtml = (html: string): string =>
    DOMPurify.sanitize(html, { ADD_ATTR: ['target'], FORBID_TAGS, FORBID_ATTR: ['style'] });

// A link taken from remote data (Hall of Fame videos, artist pages), or null when it is
// not a plain https address. Svelte puts any string into an href, javascript: included.
export const SafeUrl = (url: string | null | undefined): string | null => {
    if (!url) return null;
    try {
        const parsed = new URL(url.trim());
        return parsed.protocol === 'https:' ? parsed.href : null;
    } catch {
        return null;
    }
};
