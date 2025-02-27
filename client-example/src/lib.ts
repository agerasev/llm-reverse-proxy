const apiKey = undefined;

interface Message {
    role: string,
    content: string,
}

interface Request {
    model: string,
    messages: Message[],
    stream?: boolean,
}

interface Response {
    choices: { message: Message, index?: number, finish_reason?: string }[],
}

interface ResponseStreamChunk {
    choices: { delta: { content: string }, index?: number, finish_reason?: string | null }[],
}

export async function chat(url: string) {
    const res = await fetch(url, {
        method: "POST",
        headers: {
            'Accept': 'application/json',
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${apiKey}`,
        },
        body: JSON.stringify({
            "model": "",
            "messages": [
                {
                    "role": "user",
                    "content": "Hello!",
                }
            ],
            "stream": true,
        })
    });
    if (!res.body) {
        return;
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder("utf-8");
    while (true) {
        const { value, done } = await reader.read();
        if (done) { break; }
        let text = decoder.decode(value);
        if (!text.startsWith("data: ")) {
            console.error(`Bad chunk data: ${text}`);
        }
        text = text.slice(6).trim();
        if (text === "[DONE]") { break; }
        const chunk: ResponseStreamChunk = JSON.parse(text);
        console.log(chunk.choices[0].delta.content);
    }
}
