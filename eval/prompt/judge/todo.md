# To-do list

Request: A to-do list: an input to add a task with an 'Add' button, a list of four tasks each with a checkbox (one already done), the task text and a delete action, and a footer showing how many tasks are left.

## Sample A

```
Page title="To-do"
  Stack
    Toolbar
      Input label="New task" placeholder="What needs doing?" value={"@path":"/newTask"} name="newTask"
      Button label="Add"
    List
    Text text={"@call":"formatString","args":{"value":"${/remaining} tasks variant="muted"
[template taskRow: repeated once per item of its list]
  Stack
    Checkbox label="" name="done"
    Text text={"@path":"title"}
    Button variant="ghost" icon="trash-2"
[data model] {"newTask":"","remaining":3,"tasks":[{"id":"t1","title":"Buy groceries","done":false},{"id":"t2","title":"Write project report","done":true},{"id":"t3","title":"Call the dentist","done":false},{"id":"t4","title":"Water the plants","done":false}]}
```

## Sample B

```
Page title="To-do"
  Card
    Stack
      Toolbar
        [slot trailing]
          Button label="Add"
        Input label="New task" placeholder="What needs doing?" value={"@path":"/newTask"} name="newTask"
      List
      Text text={"@call":"concat","args":{"values":[{"@path":"/left"}," task variant="muted"
[template item: repeated once per item of its list]
  Stack
    Checkbox label={"@path":"text"} name="done"
    Button label="Delete" variant="ghost" icon="trash-2"
[data model] / = {"newTask":"","left":3,"tasks":[{"id":"t1","text":"Buy groceries","done":true},{"id":"t2","text":"Write report","done":false},{"id":"t3","text":"Call the dentist","done":false},{"id":"t4","text":"Water the plants","done":false}]}
```
