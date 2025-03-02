import { OpenAI } from 'openai';
import { Stream } from 'openai/streaming';
import {
    ChatCompletionChunk,
    ChatCompletionAssistantMessageParam,
    ChatCompletionUserMessageParam,
} from 'openai/resources/chat/completions';

export interface Message {
    role: "user" | "assistant";
    content: string;
}

export class Client {
    client: OpenAI;

    constructor(baseURL: string, apiKey: string = "") {
        this.client = new OpenAI({
            baseURL,
            apiKey,
            dangerouslyAllowBrowser: true,
        });
    }

    async send(model: string, messages: Message[]): Promise<ChatStream> {
        const stream = await this.client.chat.completions.create({
            model,
            messages: messages as (ChatCompletionUserMessageParam | ChatCompletionAssistantMessageParam)[],
            stream: true,
        });

        return new ChatStream(stream);
    }
}

export class ChatStream implements AsyncIterable<string> {
    stream: Stream<ChatCompletionChunk>;
    message: Message;
    ended: boolean;

    constructor(stream: Stream<ChatCompletionChunk>) {
        this.stream = stream;
        this.message = { role: "assistant", content: "" };
        this.ended = false;
    }

    [Symbol.asyncIterator](): AsyncIterator<string> {
        return this.extract();
    }

    private async * extract() {
        for await (const chunk of this.stream) {
            const content = chunk.choices[0]?.delta?.content || '';
            this.message.content += content;
            yield content;
        }
        this.ended = true;
    }

    async collect(): Promise<Message> {
        if (!this.ended) {
            for await (const _ of this.extract()) { }
        }
        return this.message;
    }
}
