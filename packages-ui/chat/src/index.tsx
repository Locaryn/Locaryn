import { useState } from "react";

export type ChatMessage = {
  role: "user" | "assistant" | "tool" | "system";
  text: string;
};

export function ChatPanel({
  messages,
  onSend,
}: {
  messages: ChatMessage[];
  onSend: (text: string) => void;
}) {
  const [input, setInput] = useState("");
  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div style={{ flex: 1, overflow: "auto", padding: 16 }}>
        {messages.map((m, i) => (
          <div
            key={i}
            style={{
              maxWidth: "80%",
              margin: "6px 0",
              padding: "10px 12px",
              borderRadius: 10,
              alignSelf: m.role === "user" ? "flex-end" : "flex-start",
              background: m.role === "user" ? "#388bfd" : "#161b22",
              color: m.role === "user" ? "#fff" : "#e6edf3",
            }}
          >
            {m.text}
          </div>
        ))}
      </div>
      <div style={{ display: "flex", gap: 8, padding: 12 }}>
        <textarea
          style={{ flex: 1, height: 60 }}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              if (input.trim()) {
                onSend(input);
                setInput("");
              }
            }
          }}
        />
        <button type="button" onClick={() => input.trim() && (onSend(input), setInput(""))}>
          Send
        </button>
      </div>
    </div>
  );
}
