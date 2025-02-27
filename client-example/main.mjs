import 'dotenv/config'
import { chat } from "./bundle.js";

// chat("http://127.0.0.1:8080/v1/chat/completions");

if (process.env.OPENAI_API_KEY) {
    chat(
        "https://api.openai.com/v1/chat/completions",
        "gpt-4o",
        process.env.OPENAI_API_KEY,
    );
} else {
    console.error("Empty OPENAI_API_KEY env var");
}
