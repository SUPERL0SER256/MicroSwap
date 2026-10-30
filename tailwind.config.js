/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        scene: {
          bg: "#0b0d11",
          card: "#13171f",
          border: "#222834",
          accent: "#60a5fa",
        },
      },
    },
  },
  plugins: [],
};
