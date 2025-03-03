import { Client, Message } from "common";

function escapeHtml(text: string) {
    let div = document.createElement('div');
    div.innerText = text;
    return div.innerHTML;
}

class Chat {
    client: Client;
    model: string;
    messages: Message[];

    constructor(model: string) {
        this.client = new Client(location.protocol + '//' + location.host);
        this.model = model;
        this.messages = [];
    }

    async send() {
        const prompt = document.getElementById("prompt")! as HTMLTextAreaElement;
        const text = prompt.value;
        prompt.value = ""
        console.log(text);

        const history = document.getElementById("history")!;
        const msg: Message = { role: "user", content: text };
        this.messages.push(msg);
        history.innerHTML += "<h3>User</h3>";
        history.innerHTML += escapeHtml(msg.content);

        let button = document.getElementById("send")! as HTMLButtonElement;
        button.disabled = true;
        const stream = await this.client.send(this.model, this.messages);

        history.innerHTML += "<h3>Assistant</h3>";
        for await (const chunk of stream) {
            history.innerHTML += escapeHtml(chunk);
        }
        this.messages.push(await stream.collect());
        button.disabled = false;
    }

    register() {
        document.getElementById("send")!.addEventListener("click", (() => this.send()).bind(this));
    }
}


function main() {
    const chat = new Chat("");
    chat.register();
}

window.addEventListener("load", main);
