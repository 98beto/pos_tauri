import type { ReactNode } from "react";

export type ViewId = "pos" | "products" | "inventory" | "sales";

export type NavigationItem = {
  id: ViewId;
  label: string;
  title: string;
  description: string;
  icon: ReactNode;
};

function Icon({ children }: { children: ReactNode }) {
  return (
    <svg viewBox="0 0 24 24" className="size-5" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
      {children}
    </svg>
  );
}

export const navigation: NavigationItem[] = [
  {
    id: "pos",
    label: "Punto de venta",
    title: "Punto de venta",
    description: "Registra una nueva venta y administra el cobro.",
    icon: <Icon><path strokeLinecap="round" strokeLinejoin="round" d="M4 5.5h2l1.5 9h9.4l1.7-6H7M9.5 19h.01M16.5 19h.01" /></Icon>,
  },
  {
    id: "products",
    label: "Productos",
    title: "Productos",
    description: "Administra el catalogo, precios y clasificaciones.",
    icon: <Icon><path strokeLinecap="round" strokeLinejoin="round" d="m4 7.5 8-4 8 4-8 4-8-4Zm0 0v9l8 4 8-4v-9M12 11.5v9" /></Icon>,
  },
  {
    id: "inventory",
    label: "Inventario",
    title: "Inventario",
    description: "Consulta existencias y movimientos de productos.",
    icon: <Icon><path strokeLinecap="round" strokeLinejoin="round" d="M4 6h16v14H4V6Zm3-3h10v3M8 11h8M8 15h5" /></Icon>,
  },
  {
    id: "sales",
    label: "Ventas",
    title: "Ventas",
    description: "Consulta tickets y el historial de operaciones.",
    icon: <Icon><path strokeLinecap="round" strokeLinejoin="round" d="M6 3h12v18l-3-2-3 2-3-2-3 2V3Zm3 5h6M9 12h6M9 16h3" /></Icon>,
  },
];
