import { render } from "preact";
import { LocationProvider, Route, Router } from "preact-iso";
import { initTheme } from "./lib/theme";
import "./index.css";
import { Header } from "./components/Header";
import Home from "./routes/index";
import Search from "./routes/search";
import History from "./routes/history";
import Dashboard from "./routes/dashboard";

export function App() {
  return (
    <LocationProvider>
      <div class="min-h-screen flex flex-col">
        {/* Hoisted: persists across routes (one /v1/models fetch) and owns
            the global settings dialog mount (?settings=open on any route). */}
        <Header />
        <Router>
          <Route path="/" component={Home} />
          <Route path="/search" component={Search} />
          <Route path="/history" component={History} />
          <Route path="/dashboard" component={Dashboard} />
        </Router>
      </div>
    </LocationProvider>
  );
}

render(<App />, document.getElementById("app")!);
initTheme();
