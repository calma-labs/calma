---
paths:
  - "app/**/*.{tsx,ts,css}"
---

## Stack
- **Tailwind CSS v4** is the only styling system. This project uses v4 syntax exclusively: design tokens live in `@theme` blocks in `globals.css`. There is **no** `tailwind.config.ts` — do not create one and do not use v3 config syntax.
- Do not introduce CSS Modules, styled-components, Emotion, Sass, or inline `style={}` (except for truly dynamic values like computed positions).
- All global styles live in `src/app/globals.css`. Never create additional `.css` files.
- UI primitives live in `src/components/ui/` (shadcn/ui pattern with `cva` variants).

## Before you style anything
1. Check `src/components/ui/` — does a primitive already exist for this? Use it.
2. Check 2–3 sibling components in the same feature — follow their established pattern.
3. Only then write new markup, using the rules below.

## Design tokens — always use, never bypass
- Colors: use semantic tokens only — `bg-background`, `text-foreground`, `bg-primary`, `text-muted-foreground`, `border-border`, `bg-destructive`.
- **Never** use raw palette classes (`bg-blue-500`, `text-gray-700`) or arbitrary hex values (`bg-[#3b82f6]`).
- If a needed color doesn't exist, add a token to `globals.css` first, then use it. Do not hardcode. Canonical way to add a token:

```css
/* globals.css */
@theme {
  --color-warning: oklch(0.8 0.15 85);
}
```
```tsx
<div className="bg-warning" />
```

## Escape hatch
If an arbitrary value is genuinely unavoidable (e.g. a third-party embed requires an exact pixel height), mark it:

```tsx
{/* style-exception: YouTube embed requires fixed 437px height */}
<iframe className="h-[437px]" />
```

Never use an arbitrary value without a `style-exception` comment. These are greppable and reviewed.

## Spacing & sizing
- Use the standard scale only: `1, 2, 3, 4, 6, 8, 12, 16, 24` (e.g. `p-4`, `gap-6`, `mt-8`).
- **Never** use arbitrary values like `p-[13px]`, `w-[347px]`, `mt-[7px]`. If pixel-perfect sizing seems required, question the design instead.
- Prefer `gap-*` on flex/grid parents over margins on children.

## Typography
- Use the type scale: `text-sm`, `text-base`, `text-lg`, `text-xl`, `text-2xl` etc. with `font-medium` / `font-semibold`. No arbitrary font sizes.
- Body text: `text-foreground`. Secondary text: `text-muted-foreground`. Never `text-gray-*`.

## Components & variants
- Reuse existing primitives (`Button`, `Card`, `Input`, `Dialog`…) before writing new markup. Check `src/components/ui/` first.
- Style variations go through `cva` variants, not one-off class overrides. If a new variant is needed, add it to the component's `cva` definition.
- Compose conditional classes with the `cn()` helper (`src/lib/utils.ts`). Never use string concatenation or template literals for classes — Tailwind can't statically detect dynamic class names.

```tsx
// ✅ Good
<div className={cn("rounded-lg border bg-card p-4", isActive && "ring-2 ring-primary")} />

// ❌ Bad
<div className={`rounded-lg p-[13px] bg-[#fff] ${isActive ? "ring-blue-500" : ""}`} />
```

## Forbidden — never import or install
- `*.module.css` imports, `<style jsx>`, or any new `.css`/`.scss` file
- styled-components, Emotion, or any CSS-in-JS library
- Component/UI libraries (MUI, Chakra, DaisyUI, Ant, Mantine) — do not add these "to save time"; ask first
- `tw-` prefixed wrapper libraries (twin.macro etc.)

## Class hygiene
- Class ordering is enforced by `prettier-plugin-tailwindcss` — run `npm run format` after style changes; never hand-order classes.
- No `@apply` in CSS files except for genuinely global patterns already established in `globals.css`. Default to utilities in JSX.

## Responsive & dark mode
- Mobile-first: base classes target mobile, add `md:` / `lg:` for larger screens. Never use `max-*:` variants unless unavoidable.
- Dark mode is handled by semantic tokens automatically. **Never** write `dark:` variants manually — if something looks wrong in dark mode, fix the token, not the component.

## Accessibility (non-negotiable)
- Interactive elements must have visible focus states — use `focus-visible:ring-2 focus-visible:ring-ring`, never `outline-none` without a replacement.
- Maintain WCAG AA contrast; semantic tokens already guarantee this — another reason not to bypass them.
- Don't convey state by color alone; pair with icons or text.

## Verification
After any styling change, run:
1. `npm run lint` — catches class order and unknown classes
2. `npm run typecheck`
3. Visually confirm both light and dark mode if tokens were touched
