Note the following updates in the libraries used in the project:

The `glib::clone!` macro now has a different syntax using attributes, like in this example:

```rs
let label = gtk::Label::new("");
btn.connect_clicked(clone!(
    #[weak(rename_to=this)]
    self,
    #[weak]
    label,
    move |btn| {
        // ...
    }
))
```