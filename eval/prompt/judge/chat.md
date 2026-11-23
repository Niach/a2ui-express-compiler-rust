# Chat composer

Request: A chat view: a scrollable list of three messages between the user and an assistant (bubbles, newest at the bottom) and a message composer pinned below with a send action.

## Sample A

```
Page title="Chat" subtitle="Assistant"
  Stack
    ScrollArea
      Stack
        MessageBubble text="Hi! Can you help me plan a weekend trip to Lisbon?"
        MessageBubble text="Of course! Do you prefer sightseeing, food, or a relaxed pace? Tell me your budget and I will draft an itinerary."
          [slot avatar]
            Avatar name="Assistant"
        MessageBubble text="Mostly food and relaxed walks, around 300 euros."
    Box
      Composer placeholder="Write a message…" value={"@path":"/draft"}
[data model] / = {"draft":""}
```

## Sample B

```
Stack
  ScrollArea
    Stack
      MessageBubble text="Hi! Can you help me plan a trip to Lisbon?"
      MessageBubble text="Of course! How many days will you be there, and what do you enjoy most: food, history or nightlife?"
      MessageBubble text="Four days, mostly food and history."
  Composer placeholder="Type a message" value={"@path":"/draft"}
[data model] {"draft":""}
```
