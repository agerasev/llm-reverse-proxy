import OpenAI from "openai";

const client = new OpenAI({
    baseURL: "http://127.0.0.1:8080",
    apiKey: "",
});

async function main() {
    const stream = await client.chat.completions.create({
        model: "",
        messages: [{ role: "user", content: "Say this is a test" }],
        stream: true,
    });
    for await (const chunk of stream) {
        process.stdout.write(chunk.choices[0]?.delta?.content || "");
    }
}

main();
