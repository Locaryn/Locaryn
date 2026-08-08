import { useState, useRef, useEffect } from "react";

export interface ParsedModel {
  id: string;
  engine: string;
  name: string;
  lang: string;
  quality: string;
}

export interface ModelPickerProps {
  selectedModel: string;
  groupedModels: Record<string, string[]>;
  onSelect: (model: string) => void;
  disabled?: boolean;
  parseModelName: (raw: string) => ParsedModel;
}

export function ModelPicker({
  selectedModel,
  groupedModels,
  onSelect,
  disabled,
  parseModelName,
}: ModelPickerProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const selectedInfo = parseModelName(selectedModel);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    if (open) document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  const groups = Object.entries(groupedModels);

  return (
    <div ref={containerRef} style={{ position: "relative" }}>
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
        style={{
          width: "100%",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          padding: "8px 12px",
          borderRadius: 8,
          border: "1px solid var(--border)",
          background: "var(--bg)",
          color: "var(--text)",
          textAlign: "left",
          cursor: disabled ? "default" : "pointer",
          opacity: disabled ? 0.6 : 1,
        }}
      >
        <div style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          <span style={{ fontWeight: 600, fontSize: 13 }}>{selectedInfo.engine}</span>
          <span style={{ color: "var(--text-faint)", marginLeft: 8, fontSize: 12 }}>
            {selectedInfo.name}
          </span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 6, flexShrink: 0 }}>
          {selectedInfo.lang !== "-" && (
            <span style={{
              fontSize: 9, padding: "1px 5px", borderRadius: 3,
              background: "rgba(255,255,255,0.05)", color: "var(--text-faint)",
              border: "1px solid var(--border)",
            }}>
              {selectedInfo.lang}
            </span>
          )}
          {selectedInfo.quality !== "-" && (
            <span style={{
              fontSize: 9, padding: "1px 5px", borderRadius: 3,
              background: selectedInfo.quality === "Clonage" || selectedInfo.quality === "Custom Voice" ? "rgba(150, 100, 255, 0.15)" : "rgba(100, 200, 150, 0.12)",
              color: selectedInfo.quality === "Clonage" || selectedInfo.quality === "Custom Voice" ? "var(--accent)" : "var(--text-faint)",
              border: "1px solid var(--border)",
            }}>
              {selectedInfo.quality}
            </span>
          )}
          <span style={{ fontSize: 10, color: "var(--text-faint)" }}>{open ? "▲" : "▼"}</span>
        </div>
      </button>

      {open && (
        <div
          style={{
            position: "absolute",
            zIndex: 10,
            left: 0,
            right: 0,
            top: "calc(100% + 4px)",
            maxHeight: 280,
            overflowY: "auto",
            background: "var(--bg)",
            border: "1px solid var(--border)",
            borderRadius: 8,
            boxShadow: "0 4px 12px rgba(0,0,0,0.12)",
          }}
        >
          {groups.length === 0 && (
            <div style={{ padding: 12, fontSize: 12, color: "var(--text-faint)" }}>Aucun modèle</div>
          )}
          {groups.map(([engine, models]) => (
            <div key={engine}>
              <div
                style={{
                  padding: "6px 12px",
                  background: "var(--bg-alt)",
                  fontSize: 11,
                  fontWeight: 600,
                  color: "var(--text-faint)",
                  position: "sticky",
                  top: 0,
                }}
              >
                {engine}
              </div>
              {models.map((m) => {
                const info = parseModelName(m);
                const isSelected = m === selectedModel;
                return (
                  <button
                    key={m}
                    type="button"
                    onClick={() => {
                      onSelect(m);
                      setOpen(false);
                    }}
                    style={{
                      width: "100%",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: 8,
                      padding: "8px 12px",
                      border: "none",
                      borderBottom: "1px solid var(--border)",
                      background: isSelected ? "rgba(100, 150, 255, 0.1)" : "transparent",
                      color: isSelected ? "var(--accent)" : "var(--text)",
                      cursor: "pointer",
                      textAlign: "left",
                    }}
                  >
                    <div
                      style={{
                        fontSize: 13,
                        fontWeight: isSelected ? 600 : 400,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {info.name}
                    </div>
                    <div style={{ display: "flex", gap: 4, flexShrink: 0 }}>
                      {info.lang !== "-" && (
                        <span
                          style={{
                            fontSize: 9,
                            padding: "1px 5px",
                            borderRadius: 3,
                            background: "rgba(255,255,255,0.05)",
                            color: "var(--text-faint)",
                            border: "1px solid var(--border)",
                          }}
                        >
                          {info.lang}
                        </span>
                      )}
                      {info.quality !== "-" && (
                        <span
                          style={{
                            fontSize: 9,
                            padding: "1px 5px",
                            borderRadius: 3,
                            background:
                              info.quality === "Clonage" || info.quality === "Custom Voice"
                                ? "rgba(150, 100, 255, 0.15)"
                                : "rgba(100, 200, 150, 0.12)",
                            color:
                              info.quality === "Clonage" || info.quality === "Custom Voice"
                                ? "var(--accent)"
                                : "var(--text-faint)",
                            border: "1px solid var(--border)",
                          }}
                        >
                          {info.quality}
                        </span>
                      )}
                    </div>
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
