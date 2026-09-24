import { EmptyState } from "../components/ui";
import type { ViewId } from "../layout/navigation";

const content: Record<ViewId, { title: string; description: string; symbol: string }> = {
  pos: {
    title: "El punto de venta estara disponible pronto",
    description: "La estructura esta lista para conectar el carrito y el flujo de cobro en la siguiente etapa.",
    symbol: "+",
  },
  products: {
    title: "Catalogo pendiente de implementar",
    description: "Aqui se mostraran los productos, marcas y categorias del negocio.",
    symbol: "P",
  },
  inventory: {
    title: "Inventario pendiente de implementar",
    description: "Aqui se mostraran las existencias y los movimientos registrados.",
    symbol: "I",
  },
  sales: {
    title: "Historial pendiente de implementar",
    description: "Aqui se mostraran los tickets y detalles de las ventas completadas.",
    symbol: "V",
  },
};

export function PlaceholderView({ view }: { view: ViewId }) {
  const state = content[view];
  return (
    <main id="main-content" className="p-4 sm:p-6 min-[900px]:p-8" tabIndex={-1}>
      <section className="min-h-[calc(100dvh-8rem)] rounded-card border border-line bg-surface">
        <EmptyState className="min-h-[calc(100dvh-8rem)]" title={state.title} description={state.description} icon={state.symbol} />
      </section>
    </main>
  );
}
