import OpenAI from 'openai';

export async function chat(baseURL: string, model: string = "", apiKey: string = "") {
    const openai = new OpenAI({
        baseURL,
        apiKey,
        dangerouslyAllowBrowser: true,
    });

    const stream = openai.beta.chat.completions.stream({
        model: model,
        messages: [{ role: 'user', content: 'Hello!' }],
        stream: true,
    });

    for await (const chunk of stream) {
        console.log(chunk.choices[0]?.delta?.content || '');
    }

    return await stream.finalChatCompletion();
}
