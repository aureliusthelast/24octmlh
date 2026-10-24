/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ['class'],
  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
  ],
  theme: {
    extend: {
      colors: {
        border: 'hsl(var(--border, 240 3.7% 15.9%))',
        input: 'hsl(var(--input, 240 3.7% 15.9%))',
        ring: 'hsl(var(--ring, 240 4.9% 83.9%))',
        background: 'hsl(var(--background, 240 10% 3.9%))',
        foreground: 'hsl(var(--foreground, 0 0% 98%))',
        primary: {
          DEFAULT: 'hsl(var(--primary, 210 100% 52%))',
          foreground: 'hsl(var(--primary-foreground, 0 0% 100%))',
        },
        secondary: {
          DEFAULT: 'hsl(var(--secondary, 240 3.7% 15.9%))',
          foreground: 'hsl(var(--secondary-foreground, 0 0% 98%))',
        },
        destructive: {
          DEFAULT: 'hsl(var(--destructive, 0 62.8% 30.6%))',
          foreground: 'hsl(var(--destructive-foreground, 0 0% 98%))',
        },
        muted: {
          DEFAULT: 'hsl(var(--muted, 240 3.7% 15.9%))',
          foreground: 'hsl(var(--muted-foreground, 240 5% 64.9%))',
        },
        accent: {
          DEFAULT: 'hsl(var(--accent, 240 3.7% 15.9%))',
          foreground: 'hsl(var(--accent-foreground, 0 0% 98%))',
        },
        card: {
          DEFAULT: 'hsl(var(--card, 240 10% 6.5%))',
          foreground: 'hsl(var(--card-foreground, 0 0% 98%))',
        },
      },
      borderRadius: {
        lg: 'var(--radius, 0.5rem)',
        md: 'calc(var(--radius, 0.5rem) - 2px)',
        sm: 'calc(var(--radius, 0.5rem) - 4px)',
      },
    },
  },
  plugins: [],
};
