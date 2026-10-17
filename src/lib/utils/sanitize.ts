import DOMPurify from 'dompurify';

// Everything rendered with {@html} goes through here. Some of that markup comes from
// outside the Hub (changelogs fetched from GitHub, a file name quoted in an error), and
// script running in this page can call the backend, which can start programs. Formatting
// tags survive; scripts, event handlers and javascript: links do not.
export const SafeHtml = (html: string): string => DOMPurify.sanitize(html, { ADD_ATTR: ['target'] });
