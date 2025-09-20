<script>
    import SecretButton from '$lib/components/SecretButton.svelte';

    const setColor = () => {
        document.getElementById("secret").style.backgroundColor = "white"
    }

    const getSecret = async () => {
        const secret = document.getElementById("secret").value;
        if (secret === "" || !secret) return;

        const secret_value = btoa(secret).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");

        const url = "https://opentaiko.neocities.org/" + secret_value + ".zip"
        const response = await fetch(url, { method: "HEAD" });

        if (response.ok) {
            console.log("Secret found!");
            document.getElementById("secret").style.backgroundColor = "rgb(150,255,150)";
            setTimeout(setColor, 2000);
            return url;
        }
        else {
            console.log("Secret not found. Created '" + secret_value + "' from '" + secret + "'.");
            document.getElementById("secret").style.backgroundColor = "rgb(255,150,150)";
            setTimeout(setColor, 2000);
            return;
        }
    }

    const fetchSecret = async () => {
        const url = await getSecret();

        if (url) {
            const downloadurl = document.createElement("a");
            downloadurl.href = url;
            downloadurl.click();
        }
    }
</script>

<div class="content">
    <div class="collection">
        <form onsubmit="event.preventDefault(); fetchSecret();">
            <input type="text" id="secret" name="secret">
        </form>
    <div style="width:fit-content;margin:auto;">
            <SecretButton
                color1={'white'}
                color2={'white'}
                textColor={'black'}
                text={'???'}
                OnClick={fetchSecret}
            />
        </div>
    </div>

    <script>
        const setColor = () => {
        document.getElementById("secret").style.backgroundColor = "white"
    }

    const getSecret = async () => {
        const secret = document.getElementById("secret").value;
        if (secret === "" || !secret) return;

        const secret_value = btoa(secret).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");

        const url = "https://opentaiko.neocities.org/" + secret_value + ".zip"
        const response = await fetch(url, { method: "HEAD" });

        if (response.ok) {
            console.log("Secret found!");
            document.getElementById("secret").style.backgroundColor = "rgb(150,255,150)";
            setTimeout(setColor, 2000);
            return url;
        }
        else {
            console.log("Secret not found. Created '" + secret_value + "' from '" + secret + "'.");
            document.getElementById("secret").style.backgroundColor = "rgb(255,150,150)";
            setTimeout(setColor, 2000);
            return;
        }
    }

    const fetchSecret = async () => {
        const url = await getSecret();

        if (url) {
            const downloadurl = document.createElement("a");
            downloadurl.href = url;
            downloadurl.target = "_blank"; 
            downloadurl.click();
        }
    }

    function disableImgDragging() {
        var images = document.getElementsByTagName("img");
        for(var i = 0 ; i < images.length ; i++) {
            images[i].classList.add('no-drag');
            images[i].setAttribute('no-drag', 'on');
            images[i].setAttribute('draggable', 'false');
            images[i].addEventListener('dragstart', function( event ) {
                event.preventDefault();
            }, false);	
        }
    }
    disableImgDragging();
    </script>
</div>

<style>
    form {
        border-width: 1px;
        border-color: #888888;
        border-radius: 2px;
    }
    .content {
        display: flex;
        align-content: center;
        color: #000000;
        background-color: #000000;
        position: absolute;
        top: 20px; bottom: 36px; left: 109px; right: 36px;
        border-width: 3px;
        border-color: #1f1f1f;
        border-radius: 2px;
        z-index: 1;
    }
    .collection {margin: auto;}
</style>