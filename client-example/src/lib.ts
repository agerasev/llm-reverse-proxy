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

export async function chat(url: string, model: string = "", apiKey: string = "") {
    const res = await fetch(url, {
        method: "POST",
        headers: {
            'Accept': 'application/json',
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${apiKey}`,
        },
        body: JSON.stringify({
            "model": model,
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

    let stop = false;
    while (!stop) {
        const { value, done } = await reader.read();
        if (done) { break; }

        const payload = decoder.decode(value);
        for (const data of payload.split("\n\n")) {
            if (!data) { break; }

            if (!data.startsWith("data: ")) {
                console.error(`Bad chunk data: ${data}`);
            }

            const text = data.slice(6).trim();
            if (text === "[DONE]") { stop = true; break; }

            const chunk: ResponseStreamChunk = JSON.parse(text);
            console.log(chunk.choices[0].delta.content);
        }
    }
}
