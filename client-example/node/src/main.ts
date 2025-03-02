import 'dotenv/config'
import { Client } from "common";

async function main(server_kind: string) {
    let client: Client;
    let model = "";
    switch (server_kind) {
        case "proxy":
            client = new Client("http://127.0.0.1:4000", "");
            break;
        case "llama_cpp":
            client = new Client("http://127.0.0.1:8080", "");
            break;
        case "openai":
            if (process.env.OPENAI_API_KEY) {
                client = new Client(
                    "https://api.openai.com/",
                    process.env.OPENAI_API_KEY,
                );
                model = "gpt-4o";
            } else {
                console.error("Empty OPENAI_API_KEY env var");
                return;
            }
            break;
        default:
            console.error(`Unknown server_kind: ${server_kind}`)
            return;
    }
    const stream = await client.send(model, [{ role: "user", content: "Hello!" }]);
    for await (const chunk of stream) {
        process.stdout.write(chunk);
    }
    process.stdout.write("\n\n");

    console.log(await stream.collect());
}

await main(process.argv[2] || "proxy");
