export interface PlaygroundExample {
  id: string;
  name: string;
  description: string;
  input: string;
}

export const PLAYGROUND_EXAMPLES: PlaygroundExample[] = [
  {
    id: "flowchart-basics",
    name: "Flowchart Basics",
    description: "Linear flow with decision branch",
    input: `graph TD
A[Start] --> B{Ready?}
B -->|Yes| C[Deploy]
B -->|No| D[Wait]
D --> B`
  },
  {
    id: "flowchart-fanout",
    name: "Fan-out",
    description: "One source branching to multiple targets",
    input: `graph LR
API --> Auth
API --> Billing
API --> Search
API --> Profile`
  },
  {
    id: "sequence-basics",
    name: "Sequence Basics",
    description: "Request/response interaction",
    input: `sequenceDiagram
    participant User
    participant App
    participant API
    User->>App: Submit form
    App->>API: POST /submit
    API-->>App: 200 OK
    App-->>User: Success`
  },
  {
    id: "sequence-retry",
    name: "Sequence Retry",
    description: "Retry flow with fallback",
    input: `sequenceDiagram
    participant Client
    participant Service
    Client->>Service: GET /resource
    Service-->>Client: 503
    Client->>Service: Retry request
    Service-->>Client: 200`
  },
  {
    id: "class-basics",
    name: "Class Basics",
    description: "Simple class relationship",
    input: `classDiagram
class Animal {
  +String name
  +eat()
}
class Dog {
  +bark()
}
Animal <|-- Dog`
  },
  {
    id: "class-interfaces",
    name: "Class Interfaces",
    description: "Interface implementation",
    input: `classDiagram
class Logger {
  <<interface>>
  +log(message)
}
class ConsoleLogger
class FileLogger
Logger <|.. ConsoleLogger
Logger <|.. FileLogger`
  }
];

export const DEFAULT_EXAMPLE_ID = "flowchart-basics";

export function findExampleById(id: string): PlaygroundExample | null {
  return PLAYGROUND_EXAMPLES.find((example) => example.id === id) ?? null;
}
