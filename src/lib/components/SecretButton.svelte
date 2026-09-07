<script>
    /**
     * @typedef {Object} Props
     * @property {string} [color1]
     * @property {string} [color2]
     * @property {any} [OnClick]
     * @property {string} [text]
     * @property {string} [textColor]
     */

    /** @type {Props} */
    let {
        color1 = "rgb(255, 133, 62)",
        color2 = "rgb(255, 102, 42)",
        OnClick = () => {},
        text = "",
        textColor = ""
    } = $props();
</script>


<button type="button"
    class="button"
    onclick={OnClick}
    style="--color1: {color1}; --color2: {color2}; --textColor: {textColor}"
>
    {text}
</button>

<style>
    .button {
        font-size: 16pt;

        width: fit-content;
        border-radius: 4px;
        margin: 5px;
        padding: 5px;
        
        --textcolor: white;
        text-decoration: none;
        cursor: pointer;

        background-image: linear-gradient(var(--color1), var(--color2));
        color: var(--textColor)
    }

    .button:active {
        background-image: linear-gradient(var(--color2), var(--color1));
    }
</style>