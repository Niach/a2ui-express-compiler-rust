# Table booking

Request: A restaurant table booking form: a date picker, a time choice (18:00, 19:00, 20:00, 21:00), party size from 1 to 8, name and phone (both required), an optional special-requests note, and a 'Book table' button.

## Sample A

```
Page title="Book a table" subtitle="Reserve your evening with us"
  Card
    Form name="booking" on=[submit→event bookTable {date: $/booking/date, time: $/booking/time, party: $/booking/party, name: $/booking/name, phone: $/booking/phone, notes: $/booking/notes}]
      FieldGroup columns=2
        DatePicker name="date" label="Date" placeholder="Pick a date" value=$/booking/date checks=[required(value=$/booking/date) "Choose a date"]
        Select name="time" label="Time" value=$/booking/time options[4: "18:00" | "19:00" | "20:00" | "21:00"]
      NumberField name="party" label="Party size" min=1 max=8 step=1 precision=0 value=$/booking/party
      Input name="name" label="Name" placeholder="Your name" value=$/booking/name checks=[required(value=$/booking/name) "Name is required"]
      Input name="phone" label="Phone" type="tel" placeholder="Phone number" value=$/booking/phone checks=[required(value=$/booking/phone) "Phone is required"]
      Textarea name="notes" label="Special requests (optional)" placeholder="Allergies, occasion, seating preference" rows=3 value=$/booking/notes
      FormActions submit="Book table"
[data model] /booking = {"date":"","time":"19:00","party":2,"name":"","phone":"","notes":""}
```

## Sample B

```
Page title="Book a table"
  Card title="Reservation details"
    Form name="booking" on=[submit→event book_table {date: $/form/date, time: $/form/time, party: $/form/party, name: $/form/name, phone: $/form/phone, notes: $/form/notes}]
      DatePicker label="Date" name="date" value=$/form/date
      Radio label="Time" name="time" options[4: "18:00" | "19:00" | "20:00" | "21:00"] value=$/form/time orientation="horizontal"
      NumberField label="Party size" name="party" value=$/form/party min=1 max=8 step=1
      Input label="Name" name="name" value=$/form/name checks=[required(value=$/form/name) "Name is required"]
      Input label="Phone" name="phone" type="tel" value=$/form/phone checks=[required(value=$/form/phone) "Phone is required"]
      Textarea label="Special requests (optional)" name="notes" rows=3 value=$/form/notes
      FormActions submit="Book table"
[data model] {"form":{"date":"","time":"19:00","party":2,"name":"","phone":"","notes":""}}
```
