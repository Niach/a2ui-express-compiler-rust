# Chat composer

Request: A chat view: a scrollable list of three messages between the user and an assistant (bubbles, newest at the bottom) and a message composer pinned below with a send action.

## Sample A

```
Box style{display, direction, height}
  ScrollArea direction="vertical" style{flex}
    Stack direction="vertical" gap="sm" style{padding}
      MessageBubble side="out" text="Hi! Can you help me plan a trip to Lisbon?" time="10:02" status="read" tail="end"
      MessageBubble side="in" author="Assistant" text="Of course. How many days are you staying, and what do you enjoy most: food, history or beaches?" time="10:02" tail="start"
      MessageBubble side="out" text="Four days, mostly food and history." time="10:03" status="sent" tail="down"
  Composer placeholder="Type a message" value=$/draft on=[submit→event send {text: $/draft}]
[data model] /draft = ""
```

## Sample B

```
Page title="Chat"
  Stack direction="vertical" gap="md"
    ScrollArea direction="vertical" maxHeight=480
      Stack direction="vertical" gap="md"
        MessageBubble text="Hi! Can you help me plan a trip to Lisbon?" side="out" time="09:41" status="read"
        MessageBubble text="Of course! How many days are you staying, and what do you enjoy most: food, history or beaches?" side="in" time="09:42" author="Assistant"
        MessageBubble text="Four days, mostly food and history." side="out" time="09:43" status="sent"
    Composer placeholder="Write a message" value=$/draft submitLabel="Send" on=[change→event draft_changed {draft: $/draft}, submit→event send_message {text: $/draft}]
[data model] {"draft":""}
```
