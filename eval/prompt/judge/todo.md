# To-do list

Request: A to-do list: an input to add a task with an 'Add' button, a list of four tasks each with a checkbox (one already done), the task text and a delete action, and a footer showing how many tasks are left.

## Sample A

```
Page title="To-do"
  Card
    Stack gap="md"
      Stack direction="horizontal" gap="sm" align="end"
        Input name="newTask" label="New task" placeholder="What needs doing?" value=$/newTask on=[submit→event addTask {text: $/newTask}] style{flex}
        Button label="Add" on=[press→event addTask {text: $/newTask}]
      List divided=true children=[template taskItem, once per item of $/tasks]
      Text variant="muted" text=concat(values=[$/left, " tasks left"])
[template taskItem: repeated once per item of its list]
  Stack direction="horizontal" gap="sm" justify="between"
    Checkbox name="done" label=$text checked=$done on=[change→event toggleTask {id: $id, done: $done}]
    Button label="Delete" icon="trash-2" variant="ghost" size="sm" on=[press→event deleteTask {id: $id}]
[data model] / = {"newTask":"","tasks":[{"id":"t1","text":"Buy groceries","done":false},{"id":"t2","text":"Write project report","done":true},{"id":"t3","text":"Call the dentist","done":false},{"id":"t4","text":"Book flights","done":false}],"left":3}
```

## Sample B

```
Page title="To-do"
  Card title="My tasks"
    Stack direction="horizontal" gap="sm" align="end"
      Input label="New task" name="task" placeholder="What needs doing?" value=$/form/task on=[submit→event add_task {task: $/form/task}]
      Button label="Add" on=[press→event add_task {task: $/form/task}]
    List divided=true children=[template item, once per item of $/tasks]
    Separator
    Text text=$/footer variant="muted"
[template item: repeated once per item of its list]
  Stack direction="horizontal" gap="sm" align="center" justify="between"
    Checkbox label=$title name="done" checked=$done on=[change→event toggle_task {id: $id}]
    Button label="Delete" variant="ghost" icon="trash-2" on=[press→event delete_task {id: $id}]
[data model] {"form":{"task":""},"footer":"3 tasks left","tasks":[{"id":"t1","title":"Buy groceries","done":false},{"id":"t2","title":"Write project report","done":true},{"id":"t3","title":"Call the dentist","done":false},{"id":"t4","title":"Book flights","done":false}]}
```
