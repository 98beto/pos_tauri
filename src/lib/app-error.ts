export type AppError = {
  category: "validation" | "conflict" | "authentication" | "not_found" | "database" | "internal";
  code: string;
  message: string;
  field?: string;
};

export function isAppError(error: unknown): error is AppError {
  if (typeof error !== "object" || error === null) return false;

  const candidate = error as Record<string, unknown>;
  return (
    typeof candidate.category === "string" &&
    typeof candidate.code === "string" &&
    typeof candidate.message === "string" &&
    candidate.message.trim().length > 0 &&
    (candidate.field === undefined || typeof candidate.field === "string")
  );
}

export function isSessionRequired(error: unknown): boolean {
  return isAppError(error) && error.code === "SESSION_REQUIRED";
}

export function errorMessage(error: unknown, fallback: string): string {
  return isAppError(error) ? error.message : fallback;
}
