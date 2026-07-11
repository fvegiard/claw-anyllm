import fs from "node:fs";
import path from "node:path";

export interface TodoItem {
  id: string;
  content: string;
  status: "pending" | "in_progress" | "completed" | "cancelled";
}

export interface TodoStore {
  todos: TodoItem[];
  updated_at: string;
}

function todosPath(cwd: string): string {
  return path.join(cwd, ".claw", "todos.json");
}

export function loadTodos(cwd: string): TodoStore {
  const file = todosPath(cwd);
  if (!fs.existsSync(file)) {
    return { todos: [], updated_at: new Date().toISOString() };
  }
  return JSON.parse(fs.readFileSync(file, "utf8")) as TodoStore;
}

export function saveTodos(cwd: string, store: TodoStore): void {
  const file = todosPath(cwd);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  store.updated_at = new Date().toISOString();
  fs.writeFileSync(file, JSON.stringify(store, null, 2));
}

export function todosIncomplete(store: TodoStore): boolean {
  return store.todos.some(
    (t) => t.status === "pending" || t.status === "in_progress",
  );
}

export function syncTodosFromToolUse(
  cwd: string,
  toolInput: Record<string, unknown>,
): void {
  const merge = toolInput.merge === true;
  const incoming = (toolInput.todos as TodoItem[]) ?? [];
  const store = merge ? loadTodos(cwd) : { todos: [], updated_at: "" };
  const byId = new Map(store.todos.map((t) => [t.id, t]));
  for (const item of incoming) {
    byId.set(item.id, item);
  }
  store.todos = [...byId.values()];
  saveTodos(cwd, store);
}
