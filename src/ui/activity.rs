use iced::widget::{column, container, text, text_input, button};
use iced::{Alignment, Color, Element, Length, Sandbox, Settings, Theme};
use iced::alignment::Horizontal;
use iced::Alignment::{Center, End, Start};

#[derive(Default)]
pub(crate) struct SetupUI {
    username: String,
    hidden_password: String,
    password: String,
    room: String,
}

#[derive(Default)]
pub(crate) struct Status {
    username: String,
    hidden_password: String,
    password: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    MatrixUserNameChanged(String),
    MatrixUserPasswordChanged(String),
    LoginButton(String),
}


impl Sandbox for SetupUI {

    type Message = Message;

    fn new() -> Self {
        SetupUI::default()
    }
    fn title(&self) -> String {
        String::from("GEMATIK DCMSHARE")
    }
    fn update(&mut self, message: Message) {
        match message {
            Message::MatrixUserNameChanged(mut username) => {
                self.username = username;
            },
            Message::MatrixUserPasswordChanged(mut password) => {
                self.hidden_password = password.chars().map(|_| '*').collect();
                if self.password.len() > password.len() {
                    self.password.truncate(password.len());
                }
                else{
                    self.password = format!("{}{}",self.password,password.chars().last().unwrap())
                }
                println!("{}",self.password);
            },
            Message::LoginButton(mut login) => {
                println!("Login");
            }
        }
    }
    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn view(&self) -> Element<Message> {

        let title = text("GEMATIK DCMSHARE")
            .size(30)
            .style(Color::from([0.5, 0.5, 0.5]));

        let matrix_user = text_input("Benutzername", &self.username)
                .on_input(Message::MatrixUserNameChanged)
                .size(15)
                .padding(5);

        let matrix_passwd = text_input("Passwort", &self.hidden_password)
            .on_input(Message::MatrixUserPasswordChanged)
            .size(15)
            .padding(5);

        let matrix_room = text_input("Room", &self.room)
            .on_input(Message::MatrixUserPasswordChanged)
            .size(15)
            .padding(5);


        let dicom_settings = text("Dicom Settings")
            .size(15)
            .horizontal_alignment(Horizontal::Left)
            .style(Color::from([0.5, 0.5, 0.5]));

        let style = button::Style::default();
        style.align_self(Alignment::Right);

        let login_button = button("Start")
            .style(Alignment::Left)
            .on_press(Message::LoginButton("nix".to_string()));


        // align left
        let mut content = column![title, matrix_user, matrix_passwd, matrix_room, login_button, dicom_settings]
            .width(300)
            .spacing(10);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(5)
            .center_x()
            .center_y()
            .into()
    }
}
