import { ArrowRight, Box, Puzzle, Shield } from "lucide-react";
import Image from "next/image";
import Link from "next/link";

export default function HomePage() {
  return (
    <main className="flex flex-1 w-full flex-col items-center justify-center py-16 text-center md:py-20">
      <div className="container relative flex flex-col items-center gap-8 px-4 md:px-6">
        {/* Hero Section */}
        <div className="space-y-4">
          <div className="flex justify-center">
            <div className="rounded-2xl border bg-card p-3 shadow-sm">
              <Image
                src="/icon.svg"
                alt="Ayiou Icon"
                width={128}
                height={128}
                priority
              />
            </div>
          </div>
          <h1 className="text-4xl font-bold tracking-tighter sm:text-5xl md:text-6xl lg:text-7xl bg-linear-to-br from-indigo-500 via-violet-500 to-pink-500 bg-clip-text text-transparent pb-2">
            Ayiou
          </h1>
          <p className="mx-auto max-w-[700px] text-muted-foreground md:text-xl/relaxed lg:text-base/relaxed xl:text-xl/relaxed">
            A modular, type-safe chat bot framework written in Rust. Designed
            for flexibility and reliability.
          </p>
        </div>

        {/* Action Buttons */}
        <div className="flex flex-wrap items-center justify-center gap-4">
          <Link
            href="/docs"
            className="inline-flex h-10 items-center justify-center rounded-md bg-primary px-8 text-sm font-medium text-primary-foreground shadow transition-colors hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
          >
            Get Started
            <ArrowRight className="ml-2 h-4 w-4" />
          </Link>
          <Link
            href="https://github.com/Ns2Kracy/Ayiou"
            target="_blank"
            rel="noreferrer"
            className="inline-flex h-10 items-center justify-center rounded-md border border-input bg-background px-8 text-sm font-medium shadow-sm transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
          >
            GitHub
          </Link>
        </div>

        {/* Features Grid */}
        <div className="grid w-full max-w-5xl grid-cols-1 gap-8 py-12 md:grid-cols-3 lg:gap-12">
          <div className="flex flex-col items-center gap-2 rounded-xl border bg-card p-6 text-card-foreground shadow-sm">
            <div className="rounded-full bg-primary/10 p-3">
              <Shield className="h-6 w-6 text-primary" />
            </div>
            <h3 className="text-xl font-bold">Type Safe</h3>
            <p className="text-sm text-muted-foreground">
              Leveraging Rust's type system for maximum reliability and
              correctness.
            </p>
          </div>
          <div className="flex flex-col items-center gap-2 rounded-xl border bg-card p-6 text-card-foreground shadow-sm">
            <div className="rounded-full bg-primary/10 p-3">
              <Box className="h-6 w-6 text-primary" />
            </div>
            <h3 className="text-xl font-bold">Modular Core</h3>
            <p className="text-sm text-muted-foreground">
              Separated core, drivers, and adapters architecture for maximum
              flexibility.
            </p>
          </div>
          <div className="flex flex-col items-center gap-2 rounded-xl border bg-card p-6 text-card-foreground shadow-sm">
            <div className="rounded-full bg-primary/10 p-3">
              <Puzzle className="h-6 w-6 text-primary" />
            </div>
            <h3 className="text-xl font-bold">OneBot V11</h3>
            <p className="text-sm text-muted-foreground">
              Native support for the OneBot V11 protocol standard out of the
              box.
            </p>
          </div>
        </div>
      </div>
    </main>
  );
}
