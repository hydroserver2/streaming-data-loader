import os
import json
import sys
import logging
import webbrowser
import subprocess
import hydroserverpy
from scheduler import DataLoaderScheduler
from hydroserverpy.schemas.data_loaders import DataLoaderPostBody
from logging.handlers import RotatingFileHandler
from appdirs import user_data_dir
from PySide6.QtCore import Qt
from PySide6.QtGui import QAction, QIcon, QPixmap
from PySide6.QtWidgets import QApplication, QMainWindow, QSystemTrayIcon, QMenu, QWidget, QVBoxLayout, QLabel, \
     QLineEdit, QHBoxLayout, QPushButton, QMessageBox


class StreamingDataLoader(QMainWindow):

    def __init__(self):
        super(StreamingDataLoader, self).__init__()

        self.service = None
        self.scheduler = None

        self.instance_name = None
        self.hydroserver_url = None
        self.hydroserver_username = None
        self.hydroserver_password = None
        self.connected = False
        self.paused = False

        self.status_action = None
        self.connection_action = None
        self.dashboard_action = None
        self.logging_action = None
        self.pause_action = None
        self.quit_action = None

        self.url_input = None
        self.instance_input = None
        self.email_input = None
        self.password_input = None

        self.assets_path = getattr(sys, '_MEIPASS', 'assets')
        self.app_dir = user_data_dir('Streaming Data Loader', 'CIROH')

        if not os.path.exists(self.app_dir):
            os.makedirs(self.app_dir)

        self.init_ui()
        self.get_settings()
        self.connect_to_hydroserver()
        self.update_gui()

        if self.connected:
            self.scheduler = DataLoaderScheduler(
                hs_api=self.service,
                instance_name=self.instance_name
            )

        if not self.connected:
            self.show()

    def init_ui(self):
        """Builds the app UI including system tray menu and connection window"""

        # System Tray Icon
        tray_icon = QSystemTrayIcon(self)
        tray_icon_image = QIcon(os.path.join(self.assets_path, 'app_icon.png'))
        tray_icon_image.setIsMask(True)
        tray_icon.setIcon(tray_icon_image)

        # System Tray Menu
        tray_menu = QMenu(self)
        self.setup_tray_menu_status(tray_menu)
        tray_menu.addSeparator()
        self.setup_tray_menu_actions(tray_menu)
        tray_menu.addSeparator()
        self.setup_tray_menu_controls(tray_menu)
        tray_icon.setContextMenu(tray_menu)
        tray_icon.show()

        # HydroServer Connection Window
        self.setWindowTitle('Streaming Data Loader')
        self.setGeometry(300, 300, 550, 550)
        self.setFixedSize(550, 550)
        central_widget = QWidget(self)
        self.setCentralWidget(central_widget)
        layout = QVBoxLayout(central_widget)
        self.setup_connection_dialog(layout)

    def setup_tray_menu_status(self, tray_menu):
        """Components to build menu status"""

        # System Tray Menu Status
        self.status_action = QAction(self)
        self.status_action.setEnabled(False)
        tray_menu.addAction(self.status_action)

    def setup_tray_menu_actions(self, tray_menu):
        """Components to build menu actions"""

        # System Tray Menu Open Connection Window
        self.connection_action = QAction('HydroServer Connection', self)
        self.connection_action.triggered.connect(lambda: self.show())
        tray_menu.addAction(self.connection_action)

        # System Tray Menu View Data Sources
        self.dashboard_action = QAction('View Data Sources', self)
        dashboard_icon = QIcon(os.path.join(self.assets_path, 'database.png'))
        dashboard_icon.setIsMask(True)
        self.dashboard_action.setIcon(dashboard_icon)
        self.dashboard_action.triggered.connect(self.open_data_sources_dashboard)
        tray_menu.addAction(self.dashboard_action)

        # System Tray Menu View Logs
        self.logging_action = QAction('View Log Output', self)
        logging_icon = QIcon(os.path.join(self.assets_path, 'description.png'))
        logging_icon.setIsMask(True)
        self.logging_action.setIcon(logging_icon)
        self.logging_action.triggered.connect(self.open_logs)
        tray_menu.addAction(self.logging_action)

    def setup_tray_menu_controls(self, tray_menu):
        """Components to build menu controls"""

        # System Tray Menu Pause/Resume App
        self.pause_action = QAction('Pause', self)
        self.pause_action.triggered.connect(self.toggle_paused)
        tray_menu.addAction(self.pause_action)

        # System Tray Menu Shut Down App
        self.quit_action = QAction('Shut Down', self)
        quit_icon = QIcon(os.path.join(self.assets_path, 'exit.png'))
        quit_icon.setIsMask(True)
        self.quit_action.setIcon(quit_icon)
        self.quit_action.triggered.connect(app.quit)
        tray_menu.addAction(self.quit_action)

    def setup_connection_dialog(self, layout):
        """Components to build connection window"""

        # HydroServer Logo
        logo_label = QLabel(self)
        logo_label.setPixmap(
            QPixmap(os.path.join(self.assets_path, 'setup_icon.png')).scaledToWidth(500, Qt.SmoothTransformation)
        )
        logo_layout = QVBoxLayout()
        logo_layout.addWidget(logo_label, alignment=Qt.AlignCenter)
        logo_layout.setContentsMargins(10, 10, 10, 10)
        layout.addLayout(logo_layout)

        # Window Settings
        label_width = 150
        input_layout = QVBoxLayout()
        input_layout.setContentsMargins(20, 20, 20, 20)

        # HydroServer URL Input
        url_box_layout = QHBoxLayout()
        url_label = QLabel(f'HydroServer URL:', self)
        url_label.setFixedWidth(label_width)
        url_box_layout.addWidget(url_label, alignment=Qt.AlignRight)
        self.url_input = QLineEdit(self)
        self.url_input.setStyleSheet('padding: 5px;')
        self.url_input.setPlaceholderText('Enter the HydroServer URL to connect to.')
        url_box_layout.addWidget(self.url_input)
        layout.addLayout(url_box_layout)

        # Instance Name Input
        instance_box_layout = QHBoxLayout()
        instance_label = QLabel(f'Instance Name:', self)
        instance_label.setFixedWidth(label_width)
        instance_box_layout.addWidget(instance_label, alignment=Qt.AlignRight)
        self.instance_input = QLineEdit(self)
        self.instance_input.setStyleSheet('padding: 5px;')
        self.instance_input.setPlaceholderText('Enter a name for this streaming data loader.')
        instance_box_layout.addWidget(self.instance_input)
        layout.addLayout(instance_box_layout)

        # HydroServer Email Input
        email_box_layout = QHBoxLayout()
        email_label = QLabel(f'HydroServer Email:', self)
        email_label.setFixedWidth(label_width)
        email_box_layout.addWidget(email_label, alignment=Qt.AlignRight)
        self.email_input = QLineEdit(self)
        self.email_input.setStyleSheet('padding: 5px;')
        self.email_input.setPlaceholderText('Enter your HydroServer email.')
        email_box_layout.addWidget(self.email_input)
        layout.addLayout(email_box_layout)

        # HydroServer Password Input
        password_box_layout = QHBoxLayout()
        password_label = QLabel(f'HydroServer Password:', self)
        password_label.setFixedWidth(label_width)
        password_box_layout.addWidget(password_label, alignment=Qt.AlignRight)
        self.password_input = QLineEdit(self)
        self.password_input.setEchoMode(getattr(QLineEdit, 'Password'))
        self.password_input.setStyleSheet('padding: 5px;')
        self.password_input.setPlaceholderText('Enter your HydroServer password.')
        password_box_layout.addWidget(self.password_input)
        layout.addLayout(password_box_layout)

        layout.addLayout(input_layout)

        # Window Actions Settings
        actions_layout = QHBoxLayout()
        actions_layout.setContentsMargins(0, 0, 20, 20)
        actions_layout.addStretch(1)

        # Confirm Button
        confirm_button = QPushButton('Confirm', self)
        confirm_button.clicked.connect(lambda: self.confirm_settings())
        confirm_button.setStyleSheet(
            'background-color: #007BFF; color: white; border: 1px solid #007BFF; border-radius: 8px; padding: 8px;'
            'hover { background-color: #0056b3; }'
        )
        confirm_button.setCursor(Qt.PointingHandCursor)
        confirm_button.setFixedSize(80, 30)
        actions_layout.addWidget(confirm_button)

        # Cancel Button
        cancel_button = QPushButton('Cancel', self)
        cancel_button.clicked.connect(lambda: self.hide())
        cancel_button.setStyleSheet(
            'border: 1px solid #707070; border-radius: 8px; padding: 8px;'
            'hover { background-color: #e0e0e0; }'
        )
        cancel_button.setCursor(Qt.PointingHandCursor)
        cancel_button.setFixedSize(80, 30)
        actions_layout.addWidget(cancel_button)

        layout.addLayout(actions_layout)

    def open_data_sources_dashboard(self):
        """Opens user's Data Sources Dashboard in a browser window"""

        webbrowser.open(f'{self.hydroserver_url}/data-sources')

    def open_logs(self):
        """Opens app log file in a text viewer"""

        subprocess.call(['open', os.path.join(self.app_dir, 'streaming_data_loader.log')])

    def toggle_paused(self):
        """Toggles whether the app is paused or not"""

        self.paused = not self.paused
        if self.connected and self.paused is True:
            self.scheduler.pause()
        elif self.connected and self.paused is False:
            self.scheduler.resume()
        self.update_gui()

    def connect_to_hydroserver(self):
        """Uses connection settings to register app on HydroServer"""

        if not all([
            self.hydroserver_url, self.instance_name, self.hydroserver_username, self.hydroserver_password
        ]):
            self.connected = False
            return 'Missing required connection parameters.'

        self.service = hydroserverpy.HydroServer(
            host=self.hydroserver_url,
            auth=(self.hydroserver_username, self.hydroserver_password)
        )

        response = self.service.data_loaders.list()

        if response.status_code == 401:
            self.connected = False
            return 'Failed to login with given username and password.'

        elif response.status_code == 403:
            self.connected = False
            return 'The given account does not have permission to access this resource.'

        elif response.status_code != 200:
            self.connected = False
            return 'Failed to retrieve account Streaming Data Loader instances.'

        data_loaders = response.data

        if self.instance_name not in [
            data_loader.name for data_loader in data_loaders
        ]:
            response = self.service.data_loaders.create(
                DataLoaderPostBody(
                    name=self.instance_name
                )
            )

            if response.status_code != 201:
                self.connected = False
                return 'Failed to register Streaming Data Loader instance.'

        self.connected = True

    def get_settings(self):
        """Get settings from settings file"""

        settings_path = os.path.join(self.app_dir, 'settings.json')
        if os.path.exists(settings_path):
            with open(settings_path, 'r') as settings_file:
                settings = json.loads(settings_file.read() or 'null') or {}
                self.hydroserver_url = settings.get('url')
                self.hydroserver_username = settings.get('username')
                self.hydroserver_password = settings.get('password')
                self.instance_name = settings.get('name')
                self.paused = settings.get('paused')

    def update_settings(
            self,
            hydroserver_url=None,
            instance_name=None,
            hydroserver_username=None,
            hydroserver_password=None,
            paused=None
    ):
        """Update settings file with new settings"""

        settings_path = os.path.join(self.app_dir, 'settings.json')
        with open(settings_path, 'w') as settings_file:
            settings_file.write(json.dumps({
                'url': hydroserver_url if hydroserver_url is not None else self.hydroserver_url,
                'name': instance_name if instance_name is not None else self.instance_name,
                'username': hydroserver_username if hydroserver_username is not None else self.hydroserver_username,
                'password': hydroserver_password if hydroserver_password is not None else self.hydroserver_password,
                'paused': paused if paused is not None else self.paused
            }))
        self.get_settings()

    def confirm_settings(self):
        """Handle the user updating connection settings"""

        if not all([
            self.url_input.text(), self.instance_input.text(), self.email_input.text(), self.password_input.text()
        ]):
            return self.show_message(
                title='Missing Required Fields',
                message='All fields are required to register the Streaming Data Loader app on HydroServer.'
            )

        self.update_settings(
            hydroserver_url=self.url_input.text(),
            instance_name=self.instance_input.text(),
            hydroserver_username=self.email_input.text(),
            hydroserver_password=self.password_input.text()
        )

        connection_message = self.connect_to_hydroserver()
        self.update_gui()

        if self.connected is False:
            return self.show_message(
                title='Connection Failed',
                message=connection_message
            )

        if self.scheduler:
            self.scheduler.terminate()

        self.scheduler = DataLoaderScheduler(
            hs_api=self.service,
            instance_name=self.instance_name
        )

        if self.paused is True:
            self.scheduler.pause()

        self.show_message(
            title='Streaming Data Loader Setup Complete',
            message='The Streaming Data Loader has been successfully registered and is now running.'
        )

        self.hide()

    @staticmethod
    def show_message(title, message):
        """Show a message window to the user"""

        message_box = QMessageBox()
        message_box.setWindowTitle(title)
        message_box.setText(message)
        message_box.exec_()

    def update_gui(self):
        """Update UI elements when settings/state changes"""

        if self.paused:
            pause_action_text = 'Resume'
            pause_action_icon = 'resume.png'
        else:
            pause_action_text = 'Pause'
            pause_action_icon = 'pause.png'

        if self.connected and not self.paused:
            status = 'Running'
            connection_icon = 'connected.png'
            data_sources_enabled = True
        elif self.connected and self.paused:
            status = 'Paused'
            connection_icon = 'connected.png'
            data_sources_enabled = True
        else:
            status = 'Not Connected'
            connection_icon = 'disconnected.png'
            data_sources_enabled = False

        self.status_action.setText(f'Status: {status}')

        connected_icon = QIcon(os.path.join(self.assets_path, connection_icon))
        connected_icon.setIsMask(True)
        self.connection_action.setIcon(connected_icon)
        self.dashboard_action.setEnabled(data_sources_enabled)

        self.pause_action.setText(pause_action_text)
        pause_icon = QIcon(os.path.join(self.assets_path, pause_action_icon))
        pause_icon.setIsMask(True)
        self.pause_action.setIcon(pause_icon)

        if self.isHidden():
            self.url_input.setText(self.hydroserver_url if self.hydroserver_url else 'https://www.hydroserver.org')
            self.instance_input.setText(self.instance_name if self.instance_name else '')
            self.email_input.setText(self.hydroserver_username if self.hydroserver_username else '')
            self.password_input.setText(self.hydroserver_password if self.hydroserver_password else '')


if __name__ == '__main__':

    hydroloader_logger = logging.getLogger('hydroloader')
    scheduler_logger = logging.getLogger('scheduler')

    stream_handler = logging.StreamHandler()
    hydroloader_logger.addHandler(stream_handler)
    scheduler_logger.addHandler(stream_handler)

    user_dir = user_data_dir('Streaming Data Loader', 'CIROH')

    if not os.path.exists(user_dir):
        os.makedirs(user_dir)

    log_path = os.path.join(user_dir, 'streaming_data_loader.log')

    log_handler = RotatingFileHandler(
        filename=log_path,
        mode='a',
        maxBytes=20 * 1024 * 1024,
        backupCount=3
    )
    hydroloader_logger.addHandler(log_handler)
    scheduler_logger.addHandler(log_handler)

    logging.basicConfig(
        format='%(asctime)s %(levelname)-8s %(message)s',
        level=logging.INFO,
        datefmt='%Y-%m-%d %H:%M:%S',
        force=True,
        handlers=[
            log_handler, stream_handler
        ]
    )

    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)
    window = StreamingDataLoader()
    sys.exit(app.exec_())
