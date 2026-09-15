import { render } from "preact";
import { LocationProvider, Route, Router } from "preact-iso";
import "./index.css";
import Home from "./routes/index";
import Search from "./routes/search";
import History from "./routes/history";
import Dashboard from "./routes/dashboard";

export function App() {
  return (
    <LocationProvider>
      <Router>
        <Route path="/" component={Home} />
        <Route path="/search" component={Search} />
        <Route path="/history" component={History} />
        <Route path="/dashboard" component={Dashboard} />
      </Router>
    </LocationProvider>
  );
}

render(<App />, document.getElementById("app")!);
