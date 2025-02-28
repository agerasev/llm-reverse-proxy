import 'dotenv/config'
import { chat } from "./bundle.js";

console.log(await chat("http://127.0.0.1:4000"));
//console.log(await chat("http://127.0.0.1:8080"));

/*
if (process.env.OPENAI_API_KEY) {
    await chat(
        "https://api.openai.com/",
        "gpt-4o",
        process.env.OPENAI_API_KEY,
    );
} else {
    console.error("Empty OPENAI_API_KEY env var");
}
*/
