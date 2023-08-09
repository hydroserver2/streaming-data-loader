import os
import sys
import json
import utils
import customtkinter as ctk
from tkinter import messagebox
from PIL import Image
from appdirs import user_data_dir
from tktooltip import ToolTip


ctk.set_appearance_mode('System')
ctk.set_default_color_theme('blue')


class AppSetup(ctk.CTk):

    def __init__(self, service_url):
        super().__init__()

        self.title('Register Streaming Data Loader')

        base_path = getattr(sys, '_MEIPASS', 'assets')

        screen_width = self.winfo_screenwidth()
        screen_height = self.winfo_screenheight()

        window_width = 530
        window_height = 480

        center_x = (screen_width / 3) - (window_width / 3)
        center_y = (screen_height / 3) - (window_height / 3)

        self.geometry(f"{window_width}x{window_height}+{int(center_x)}+{int(center_y)}")
        self.resizable(width=False, height=False)
        self.protocol('WM_DELETE_WINDOW', self.exit_window)

        self.app_logo = Image.open(os.path.join(base_path, 'setup_icon.png'))
        self.app_logo = ctk.CTkImage(self.app_logo, size=(466, 200))
        self.logo_display = ctk.CTkLabel(self, text='', image=self.app_logo, corner_radius=7)
        self.logo_display.image = self.app_logo
        self.logo_display.grid(row=0, column=0)

        self.hydroserver_url = service_url
        self.callback = None

        self.setup_frame = ctk.CTkFrame(self, corner_radius=10)
        self.setup_frame.grid(row=1, column=0, padx=15, pady=20)

        self.label_loader_name = ctk.CTkLabel(
            self.setup_frame, text='Instance Name:', width=30, height=25, corner_radius=7
        )
        self.label_loader_name.grid(row=0, column=0, padx=10, pady=20, sticky='w')

        self.entry_loader_name = ctk.CTkEntry(
            self.setup_frame, placeholder_text='Enter a name for this data loader.', width=300, height=30,
            border_width=2, corner_radius=10
        )
        self.entry_loader_name.grid(row=0, column=1, padx=10, columnspan=2, sticky='w')
        ToolTip(
            self.entry_loader_name,
            msg='This is the name you will use to identify this Data Loader instance on HydroServer while setting ' +
                'up data sources.',
            delay=0.2
        )

        self.label_username = ctk.CTkLabel(
            self.setup_frame, text='HydroServer Email:', width=30, height=25, corner_radius=7
        )
        self.label_username.grid(row=1, column=0, padx=10, pady=20, sticky='w')

        self.entry_username = ctk.CTkEntry(
            self.setup_frame, placeholder_text='Enter your HydroServer email.', width=300, height=30, border_width=2,
            corner_radius=10
        )
        self.entry_username.grid(row=1, column=1, padx=10, columnspan=3, sticky='w')

        self.label_password = ctk.CTkLabel(
            self.setup_frame, text='HydroServer Password:', width=30, height=25, corner_radius=7
        )
        self.label_password.grid(row=2, column=0, padx=10, pady=20)

        self.entry_password = ctk.CTkEntry(
            self.setup_frame, placeholder_text='Enter your HydroServer password.', width=300, height=30, border_width=2,
            corner_radius=10, show='•'
        )
        self.entry_password.grid(row=2, column=1, padx=10, columnspan=3, sticky='w')

        self.button_confirm = ctk.CTkButton(self, text='Confirm', width=70, command=self.confirm_setup)
        self.button_confirm.grid(row=2, column=0, padx=100, sticky='e')

        self.button_cancel = ctk.CTkButton(
            self, text='Cancel', width=70, fg_color='gray74', hover_color='#EEE', text_color='#000',
            command=self.exit_window
        )
        self.button_cancel.grid(row=2, column=0, padx=20, sticky='e')

    def exit_window(self):
        self.destroy()

    def confirm_setup(self):

        self.button_confirm.configure(state='disabled')
        self.config(cursor='watch')

        instance = self.entry_loader_name.get()
        username = self.entry_username.get()
        password = self.entry_password.get()

        success, message = utils.sync_data_loader(
            url=self.hydroserver_url,
            name=instance,
            username=username,
            password=password
        )

        if success is False:
            return self.display_setup_error(message)

        app_dir = user_data_dir('Streaming Data Loader', 'CIROH')

        try:
            if not os.path.exists(app_dir):
                os.makedirs(app_dir)

            with open(os.path.join(app_dir, 'settings.json'), 'w') as settings_file:
                settings_file.write(json.dumps({
                    'instance': instance,
                    'username': username,
                    'password': password
                }))
        except (OSError, ValueError):
            return self.display_setup_error(
                'Failed to save account settings.'
            )

        return self.display_setup_success()

    def display_setup_success(self):
        messagebox.showinfo(
            title='Setup Complete',
            message=(
                'The Streaming Data Loader has been successfully registered and is now running.'
            )
        )
        self.callback()

    def display_setup_error(self, message):
        messagebox.showinfo(
            title='Setup Error',
            message=message
        )
        self.button_confirm.configure(state='normal')
        self.config(cursor='')
