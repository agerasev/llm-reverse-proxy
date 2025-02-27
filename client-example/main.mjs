import 'dotenv/config'
import { chat } from "./bundle.js";

await chat("http://127.0.0.1:4000/chat/completions");

/*
if (process.env.OPENAI_API_KEY) {
    await chat(
        "https://api.openai.com/v1/chat/completions",
        "gpt-4o",
        process.env.OPENAI_API_KEY,
    );
} else {
    console.error("Empty OPENAI_API_KEY env var");
}
*/
