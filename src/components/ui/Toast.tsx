import type { HTMLAttributes, ReactNode } from "react";

export type ToastProps = Omit<HTMLAttributes<HTMLDivElement>, "title"> & {
  open: boolean;
  title?: ReactNode;
  message: ReactNode;
  tone?: "success" | "info" | "error";
};

const toneClasses = {
  success: "border-[#cbd5f2] bg-[#f4f7ff] text-brand-dark",
  info: "border-info/20 bg-info-soft text-info",
  error: "border-danger/20 bg-danger-soft text-danger",
};

const marks = { success: "✓", info: "i", error: "!" };

export function Toast({ open, title, message, tone = "success", className = "", ...props }: ToastProps) {
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      aria-live={tone === "error" ? "assertive" : "polite"}
      aria-atomic="true"
      className={`fixed right-4 bottom-4 z-80 flex w-[calc(100%-2rem)] max-w-sm items-center gap-3 rounded-xl border p-4 text-sm font-semibold shadow-panel transition duration-200 sm:right-6 sm:bottom-6 ${toneClasses[tone]} ${open ? "translate-y-0 opacity-100" : "pointer-events-none translate-y-3 opacity-0"} ${className}`}
      {...props}
    >
      <span className="grid size-7 shrink-0 place-items-center rounded-full bg-current/10 font-extrabold" aria-hidden="true">
        {marks[tone]}
      </span>
      <span>
        {title && <strong className="block">{title}</strong>}
        <span className={title ? "mt-0.5 block font-medium" : "block"}>{open ? message : ""}</span>
      </span>
    </div>
  );
}
