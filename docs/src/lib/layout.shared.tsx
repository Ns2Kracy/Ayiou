import type { BaseLayoutProps } from "fumadocs-ui/layouts/shared";
import Image from "next/image";

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: (
        <div className="flex items-center gap-2">
          <Image
            src="/icon.svg"
            alt="Ayiou Logo"
            width={24}
            height={24}
            className="rounded-sm"
          />
          <span className="font-bold">Ayiou</span>
        </div>
      ),
    },
    links: [
      {
        text: "Documentation",
        url: "/docs",
        active: "nested-url",
      },
    ],
    githubUrl: "https://github.com/Ns2Kracy/Ayiou",
  };
}
