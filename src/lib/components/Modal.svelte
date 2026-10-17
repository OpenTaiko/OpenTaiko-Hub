<script module lang="ts">
    // Open dialogs, innermost last: only the top one answers Escape and keeps Tab
    // inside it (the build picker opens on top of the instance manager).
    const open: symbol[] = [];
</script>

<script lang="ts">
    // Every dialog of the Hub. It is announced as a dialog, moves focus into itself and
    // gives it back on close, keeps Tab inside, and closes on Escape or a click beside
    // it, unless Busy: a dialog applying something must not vanish halfway.
    import { onMount, type Snippet } from 'svelte';

    interface Props {
        /** Read out by screen readers; usually the dialog's title. */
        Label: string;
        OnClose: () => void;
        Busy?: boolean;
        /** Classes for the dialog card (size, padding). */
        class?: string;
        children: Snippet;
    }

    let { Label, OnClose, Busy = false, class: cardClass = '', children }: Props = $props();

    const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
    const id = Symbol();
    let card = $state<HTMLDivElement | null>(null);

    const close = () => {
        if (!Busy) OnClose();
    };

    const focusables = (): HTMLElement[] =>
        card ? [...card.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null) : [];

    onMount(() => {
        const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        open.push(id);
        // A field inside may already have taken focus (a name prompt focuses its input)
        if (card && !card.contains(document.activeElement)) (focusables()[0] ?? card).focus();
        return () => {
            open.splice(open.indexOf(id), 1);
            previous?.focus();
        };
    });

    const onKeydown = (event: KeyboardEvent) => {
        if (open[open.length - 1] !== id || !card) return;
        if (event.key === 'Escape') {
            event.preventDefault();
            close();
            return;
        }
        if (event.key !== 'Tab') return;
        const items = focusables();
        if (items.length === 0) {
            event.preventDefault();
            card.focus();
            return;
        }
        const first = items[0];
        const last = items[items.length - 1];
        const inside = card.contains(document.activeElement);
        if (event.shiftKey && (!inside || document.activeElement === first)) {
            event.preventDefault();
            last.focus();
        } else if (!event.shiftKey && (!inside || document.activeElement === last)) {
            event.preventDefault();
            first.focus();
        }
    };
</script>

<svelte:window onkeydown={onKeydown} />

<!-- The backdrop only closes on a click that lands on it, never one inside the card.
     Keyboard users close with Escape, handled above. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="modal-backdrop" onclick={(event) => { if (event.target === event.currentTarget) close(); }}>
    <div bind:this={card} class="card modal-card {cardClass}" role="dialog" aria-modal="true" aria-label={Label} tabindex="-1">
        {@render children()}
    </div>
</div>

<style>
    .modal-backdrop {
        position: fixed;
        inset: 0;
        z-index: 9999;
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 1rem;
        background: rgb(0 0 0 / 0.6);
    }
    .modal-card {
        max-height: 85vh;
        overflow-y: auto;
        outline: none;
    }
</style>
