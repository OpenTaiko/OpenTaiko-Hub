declare global {
    interface Window {
        /** Set by app.html: stops reporting errors as a failure to start. */
        __hubStarted?: () => void;
    }
}

export {};
