<script>

export let Url = "";
export let ImageSrc = "";
export let CardTitle = "";
export let CardSubtitle = "";
export let CardText = "";

</script>

<a class="card bg-initial card-hover overflow-hidden no-underline" href={Url} target="_blank">
    <header>
        <img src={ImageSrc} class="bg-black/50 w-full aspect-[21/9]" alt="Post">
    </header> 
    <div class="p-4 space-y-4 text-black dark:text-white">
        <h6 class="h6" data-toc-ignore="">{CardTitle}</h6> 
        <h3 class="h3" data-toc-ignore="">{CardSubtitle}</h3> 
        <article>
            <p>{CardText}
            </p>
        </article>
    </div> 
</a>

<style>
</style>