<script>
    export let color1 = "rgb(255, 133, 62)";
    export let color2 = "rgb(255, 102, 42)";
    export let OnClick = () => {};
    export let text = "";
    export let textColor = "";

    let isActive = false;
</script>


<!-- svelte-ignore a11y-click-events-have-key-events -->
<div 
    class="button"
    on:click={OnClick}
    style="background-image: linear-gradient({isActive ? color2 : color1}, {isActive ? color1 : color2}); color:{textColor}"
    on:mousedown={() => (isActive = true)}
    on:mouseup={() => (isActive = false)}
    on:mouseleave={() => (isActive = false)}
>
    {text}
</div>

<style>
    .button {
        font-size: 16pt;

        width: fit-content;
        border-radius: 4px;
        margin: 5px;
        padding: 5px;
        
        color: white;
        text-decoration: none;
        cursor: pointer;
    }
</style>